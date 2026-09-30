//! Port of `src/core/game/MotionPlans.ts` — the `Uint32Array` wire format for
//! army/train motion plans, shared between the simulation and the transport.
//!
//! Parity contract:
//!
//! * **Two-pass pack.** Pass 1 sums the word budget (allocation size uses the
//!   *unwrapped* `2 + 5 + pathLen` JS number); pass 2 fills the buffer and the
//!   final `offset !== out.length` guard mirrors the TS `throw`.
//! * **`>>> 0` on every field.** Non-integer/negative inputs are clamped via
//!   [`crate::jsnum::to_uint32`], exactly as a `Uint32Array` element write does.
//!   The stored `wordCount` is the *wrapped* `>>> 0` of the sum, computed in
//!   `f64` to match JS before coercion.
//! * **`unpack` control flow.** A `break` inside a `switch` case exits the
//!   `switch` only — `offset += wordCount` still runs and the loop continues.
//!   Only the top-level `wordCount < 2 || offset + wordCount > length` guard
//!   breaks the *loop*. `expectedWordCount !== wordCount` skips the record but
//!   still advances the offset. `wordCount >= 2` guarantees forward progress.

use crate::jsnum::to_uint32;

/// `PackedMotionPlanKind.GridPathSet`.
pub const PACKED_GRID_PATH_SET: u32 = 1;
/// `PackedMotionPlanKind.TrainRailPathSet`.
pub const PACKED_TRAIN_RAIL_PATH_SET: u32 = 2;

/// A record handed to [`pack_motion_plans`]. Scalar fields and `path` /
/// `car_unit_ids` elements are `f64` so the `>>> 0` coercion is observable,
/// matching the TS `number` / `TileRef` inputs.
pub enum MotionPlanInput<'a> {
    Grid {
        unit_id: f64,
        plan_id: f64,
        start_tick: f64,
        ticks_per_step: f64,
        path: &'a [f64],
    },
    Train {
        engine_unit_id: f64,
        car_unit_ids: &'a [f64],
        plan_id: f64,
        start_tick: f64,
        speed: f64,
        spacing: f64,
        path: &'a [f64],
    },
}

/// A record produced by [`unpack_motion_plans`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MotionPlanRecord {
    Grid {
        unit_id: u32,
        plan_id: u32,
        start_tick: u32,
        ticks_per_step: u32,
        path: Vec<u32>,
    },
    Train {
        engine_unit_id: u32,
        car_unit_ids: Vec<u32>,
        plan_id: u32,
        start_tick: u32,
        speed: u32,
        spacing: u32,
        path: Vec<u32>,
    },
}

/// `packMotionPlans(records)`.
pub fn pack_motion_plans(records: &[MotionPlanInput<'_>]) -> Vec<u32> {
    let mut total_words: usize = 1;
    for r in records {
        match r {
            MotionPlanInput::Grid { path, .. } => {
                let path_len = to_uint32(path.len() as f64) as usize;
                total_words += 2 + 5 + path_len;
            }
            MotionPlanInput::Train { car_unit_ids, path, .. } => {
                let car_count = to_uint32(car_unit_ids.len() as f64) as usize;
                let path_len = to_uint32(path.len() as f64) as usize;
                total_words += 2 + 7 + car_count + path_len;
            }
        }
    }

    let mut out = vec![0u32; total_words];
    out[0] = to_uint32(records.len() as f64);

    let mut offset: usize = 1;
    for r in records {
        match r {
            MotionPlanInput::Grid {
                unit_id,
                plan_id,
                start_tick,
                ticks_per_step,
                path,
            } => {
                let path_len = to_uint32(path.len() as f64);
                let word_count = to_uint32(2.0 + 5.0 + path_len as f64);
                out[offset] = PACKED_GRID_PATH_SET;
                offset += 1;
                out[offset] = word_count;
                offset += 1;
                out[offset] = to_uint32(*unit_id);
                offset += 1;
                out[offset] = to_uint32(*plan_id);
                offset += 1;
                out[offset] = to_uint32(*start_tick);
                offset += 1;
                out[offset] = to_uint32(*ticks_per_step);
                offset += 1;
                out[offset] = path_len;
                offset += 1;
                for p in path.iter() {
                    out[offset] = to_uint32(*p);
                    offset += 1;
                }
            }
            MotionPlanInput::Train {
                engine_unit_id,
                car_unit_ids,
                plan_id,
                start_tick,
                speed,
                spacing,
                path,
            } => {
                let car_count = to_uint32(car_unit_ids.len() as f64);
                let path_len = to_uint32(path.len() as f64);
                let word_count = to_uint32(2.0 + 7.0 + car_count as f64 + path_len as f64);
                out[offset] = PACKED_TRAIN_RAIL_PATH_SET;
                offset += 1;
                out[offset] = word_count;
                offset += 1;
                out[offset] = to_uint32(*engine_unit_id);
                offset += 1;
                out[offset] = to_uint32(*plan_id);
                offset += 1;
                out[offset] = to_uint32(*start_tick);
                offset += 1;
                out[offset] = to_uint32(*speed);
                offset += 1;
                out[offset] = to_uint32(*spacing);
                offset += 1;
                out[offset] = car_count;
                offset += 1;
                out[offset] = path_len;
                offset += 1;
                for c in car_unit_ids.iter() {
                    out[offset] = to_uint32(*c);
                    offset += 1;
                }
                for p in path.iter() {
                    out[offset] = to_uint32(*p);
                    offset += 1;
                }
            }
        }
    }

    assert!(
        offset == out.len(),
        "packMotionPlans size mismatch: wrote {offset}, expected {}",
        out.len()
    );
    out
}

/// `unpackMotionPlans(packed)`.
pub fn unpack_motion_plans(packed: &[u32]) -> Vec<MotionPlanRecord> {
    if packed.is_empty() {
        return Vec::new();
    }

    let record_count = packed[0];
    let mut records: Vec<MotionPlanRecord> = Vec::new();
    let mut offset: usize = 1;
    let mut i: u32 = 0;

    while i < record_count && offset + 1 < packed.len() {
        let kind = packed[offset];
        let word_count = packed[offset + 1];

        if word_count < 2 || offset + word_count as usize > packed.len() {
            break;
        }

        match kind {
            PACKED_GRID_PATH_SET => {
                if word_count >= 2 + 5 {
                    let unit_id = packed[offset + 2];
                    let plan_id = packed[offset + 3];
                    let start_tick = packed[offset + 4];
                    let ticks_per_step = packed[offset + 5];
                    let path_len = packed[offset + 6];
                    let expected = 2u64 + 5 + path_len as u64;
                    if expected == word_count as u64 {
                        let path_start = offset + 7;
                        let path_end = path_start + path_len as usize;
                        let path = packed[path_start..path_end].to_vec();
                        records.push(MotionPlanRecord::Grid {
                            unit_id,
                            plan_id,
                            start_tick,
                            ticks_per_step,
                            path,
                        });
                    }
                }
            }
            PACKED_TRAIN_RAIL_PATH_SET => {
                if word_count >= 2 + 7 {
                    let engine_unit_id = packed[offset + 2];
                    let plan_id = packed[offset + 3];
                    let start_tick = packed[offset + 4];
                    let speed = packed[offset + 5];
                    let spacing = packed[offset + 6];
                    let car_count = packed[offset + 7];
                    let path_len = packed[offset + 8];
                    let expected = 2u64 + 7 + car_count as u64 + path_len as u64;
                    if expected == word_count as u64 {
                        let car_start = offset + 9;
                        let car_end = car_start + car_count as usize;
                        let path_start = car_end;
                        let path_end = path_start + path_len as usize;
                        let car_unit_ids = packed[car_start..car_end].to_vec();
                        let path = packed[path_start..path_end].to_vec();
                        records.push(MotionPlanRecord::Train {
                            engine_unit_id,
                            car_unit_ids,
                            plan_id,
                            start_tick,
                            speed,
                            spacing,
                            path,
                        });
                    }
                }
            }
            _ => {}
        }

        offset += word_count as usize;
        i += 1;
    }

    records
}
