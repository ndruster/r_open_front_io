//! Port of the CPU half of `src/client/render/gl/utils/Affiliation.ts`:
//! the `AffiliationPalette` 4096×2 RGBA8 `cpuData` rebuild, the dirty latch
//! and the input caches. The GL plumbing (`createTexture2D` upload,
//! `texSubImage2D` flush, `deleteTexture` dispose) is host-bound and stubbed
//! out of the capture; only the byte buffer and the flags are ported.
//!
//! Faithfulness notes (quirk list):
//!
//! * `to255 = Math.round(v * 255)` (half-up): the render-settings defaults
//!   0.502 → 128.01 → 128; a NaN channel → NaN → the `Uint8Array` write
//!   stores 0; `2` → 510 wraps mod 256 to 254; `-0.5` → -128 wraps to 128.
//! * The relation gate `rel && lp > 0 && owner > 0 && owner < rs && lp < rs`:
//!   an EMPTY `Uint8Array` is truthy (objects always are) so the gate runs
//!   and every read is OOB `undefined`; `rs` NaN fails both comparisons.
//! * `relation = rel[lp * rs + owner]` — a fractional index (fractional `lp`
//!   or `rs`) is not a canonical index and reads `undefined`, which behaves
//!   exactly like `0` through the strict `=== 1` / `=== 2` gates (neutral
//!   border, enemy unit row).
//! * `setLocalPlayer` early-returns on strict `id === localPlayerID` (`-0`
//!   equals `0`, `NaN` equals nothing), so a repeat set leaves `dirty`
//!   untouched; `updateRelations` ALWAYS rebuilds.
//! * The constructor rebuilds (spectator defaults: owner 0 transparent,
//!   every other owner neutral border / enemy unit) and then clears `dirty`
//!   — the initial upload baked it in.
//! * `isSelf` is `owner > 0 && owner === lp` — a fractional / NaN `lp` never
//!   matches; owner 0 takes the transparent branch before the self test.
//! * Row 1 (`(TEX_W + owner) * 4`) has no neutral state: relation 0, 2, 3,
//!   … and `undefined` all render enemy; only 1 renders ally.

use crate::jsnum::{js_round, to_uint8};

const TEX_W: usize = 4096;
const TEX_H: usize = 2;
const DATA_LEN: usize = TEX_W * TEX_H * 4;

/// The ported `AffiliationPalette` object (CPU state only).
#[derive(Clone, Debug)]
struct Pal {
    /// The 12 `settings.affiliation` channels in TS read order:
    /// selfR/G/B, allyR/G/B, neutralR/G/B, enemyR/G/B.
    affiliation: [f64; 12],
    cpu: Vec<u8>,
    dirty: bool,
    local_player_id: f64,
    relation_data: Option<Vec<u8>>,
    relation_size: f64,
}

impl Pal {
    fn new(affiliation: [f64; 12]) -> Pal {
        let mut p = Pal {
            affiliation,
            cpu: vec![0u8; DATA_LEN],
            dirty: false,
            local_player_id: 0.0,
            relation_data: None,
            relation_size: 0.0,
        };
        p.rebuild();
        p.dirty = false; // already baked into the initial upload
        p
    }

    fn rebuild(&mut self) {
        let a = self.affiliation;
        let to255 = |v: f64| js_round(v * 255.0);
        let self_rgb = [to255(a[0]), to255(a[1]), to255(a[2])];
        let ally_rgb = [to255(a[3]), to255(a[4]), to255(a[5])];
        let neutral_rgb = [to255(a[6]), to255(a[7]), to255(a[8])];
        let enemy_rgb = [to255(a[9]), to255(a[10]), to255(a[11])];

        let lp = self.local_player_id;
        let rs = self.relation_size;
        let rel = &self.relation_data;

        for owner in 0..TEX_W {
            let mut relation = 0.0f64;
            if let Some(r) = rel {
                if lp > 0.0 && owner > 0 && (owner as f64) < rs && lp < rs {
                    let idx = lp * rs + owner as f64;
                    relation = if idx.is_finite()
                        && idx.fract() == 0.0
                        && idx >= 0.0
                        && (idx as usize) < r.len()
                    {
                        r[idx as usize] as f64
                    } else {
                        0.0 // undefined read behaves exactly like neutral here
                    };
                }
            }
            let is_self = owner > 0 && (owner as f64) == lp;

            let rgb = if owner == 0 {
                [0.0, 0.0, 0.0, 0.0]
            } else if is_self {
                [self_rgb[0], self_rgb[1], self_rgb[2], 255.0]
            } else if relation == 1.0 {
                [ally_rgb[0], ally_rgb[1], ally_rgb[2], 255.0]
            } else if relation == 2.0 {
                [enemy_rgb[0], enemy_rgb[1], enemy_rgb[2], 255.0]
            } else {
                [neutral_rgb[0], neutral_rgb[1], neutral_rgb[2], 255.0]
            };
            let boff = owner * 4;
            for (k, &v) in rgb.iter().enumerate() {
                self.cpu[boff + k] = to_uint8(v);
            }

            let rgb = if owner == 0 {
                [0.0, 0.0, 0.0, 0.0]
            } else if is_self {
                [self_rgb[0], self_rgb[1], self_rgb[2], 255.0]
            } else if relation == 1.0 {
                [ally_rgb[0], ally_rgb[1], ally_rgb[2], 255.0]
            } else {
                [enemy_rgb[0], enemy_rgb[1], enemy_rgb[2], 255.0]
            };
            let uoff = (TEX_W + owner) * 4;
            for (k, &v) in rgb.iter().enumerate() {
                self.cpu[uoff + k] = to_uint8(v);
            }
        }

        self.dirty = true;
    }
}

// kind table (matches `RigHarness::run_op`):
// 0 construct [selfR..enemyB (12)] -> dumpState
// 1 setLocalPlayer [id] -> [dirty]
// 2 updateRelations [n, size, (data)*n] -> [dirty] (n < 0 models null data)
// 3 flush -> [dirtyBefore]
// 4 dumpSlice [start, len] -> [bytes] (OOB reads -> NaN)
// 5 dumpState -> [localPlayerID, relationSize, hasData, dataLen, dirty]
#[derive(Default)]
pub struct RigHarness {
    pal: Option<Pal>,
}

impl RigHarness {
    pub fn new() -> Self {
        Self { pal: None }
    }

    pub fn reset(&mut self) {
        self.pal = None;
    }

    pub fn run_op(&mut self, kind: u8, args: &[f64]) -> Vec<f64> {
        match kind {
            0 => {
                let mut a = [0.0f64; 12];
                a.copy_from_slice(&args[0..12]);
                self.pal = Some(Pal::new(a));
                vec![
                    0.0, // localPlayerID
                    0.0, // relationSize
                    0.0, // hasData
                    -1.0, // dataLen
                    0.0, // dirty (the ctor clears it)
                ]
            }
            1 => {
                let p = self.pal.as_mut().expect("afp: construct first");
                let id = args[0];
                if id != p.local_player_id {
                    p.local_player_id = id;
                    p.rebuild();
                }
                vec![p.dirty as u8 as f64]
            }
            2 => {
                let p = self.pal.as_mut().expect("afp: construct first");
                // `n < 0` models `updateRelations(null, size)` — the field
                // becomes falsy and the relation gate never runs.
                let n = args[0];
                if n < 0.0 {
                    p.relation_data = None;
                    p.relation_size = args[1];
                } else {
                    let n = n as usize;
                    p.relation_data =
                        Some(args[2..2 + n].iter().map(|&v| to_uint8(v)).collect());
                    p.relation_size = args[1];
                }
                p.rebuild();
                vec![if p.dirty { 1.0 } else { 0.0 }]
            }
            3 => {
                let p = self.pal.as_mut().expect("afp: construct first");
                let before = p.dirty as u8 as f64;
                if p.dirty {
                    p.dirty = false;
                }
                vec![before]
            }
            4 => {
                let p = self.pal.as_ref().expect("afp: construct first");
                let start = args[0] as usize;
                let len = args[1] as usize;
                (0..len)
                    .map(|i| match p.cpu.get(start + i) {
                        Some(&b) => b as f64,
                        None => f64::NAN, // typed-array OOB read -> undefined
                    })
                    .collect()
            }
            5 => {
                let p = self.pal.as_ref().expect("afp: construct first");
                vec![
                    p.local_player_id,
                    p.relation_size,
                    if p.relation_data.is_some() { 1.0 } else { 0.0 },
                    p.relation_data
                        .as_ref()
                        .map_or(-1.0, |r| r.len() as f64),
                    p.dirty as u8 as f64,
                ]
            }
            k => unreachable!("affiliation_palette: unknown op kind {k}"),
        }
    }
}
