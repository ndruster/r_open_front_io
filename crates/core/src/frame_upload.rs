//! Port of `src/client/render/frame/Upload.ts` — `uploadFrameData(view,
//! frame)`, the pure dispatch loop over the `FrameUploadTarget` interface.
//!
//! The GPU view is modelled as a facade trace: every method call appends
//! `(methodId, numeric params…)` in TS declaration order of the interface
//! (0 uploadTileAndTrailState, 1 uploadLiveDelta, 2 uploadLiveTrailDelta,
//! 3 updateSpiralRibbons, 4 uploadRailroadState, 5 applyRailroadDust,
//! 6 updateUnits, 7 updateStructures, 8 applyDeadUnits, 9 applyConquestEvents,
//! 10 applyBonusEvents, 11 updateAttackRings, 12 updateNukeTelegraphs,
//! 13 updateNames, 14 updateRelations, 15 setSAMAllianceClusters). The
//! trace pins the exact gate sequence; the array payloads ride as lengths
//! only (the port is about the dispatch, not the buffers).
//!
//! Faithfulness notes (quirk list):
//!
//! * `if (frame.changedTiles)` is a TRUTHY gate — the empty array `[]` is
//!   truthy (enters the branch, then the `length > 0` gate skips the delta
//!   upload), while `null` / `undefined` / `""` are falsy (else branch: the
//!   full `uploadTileAndTrailState`). The wire tags it: `0` = falsy, `1` =
//!   array (with its length).
//! * `frame.trailDirtyRowMax >= 0` is a numeric compare — `NaN >= 0` is
//!   false, `-0 >= 0` is true. It lives INSIDE the truthy branch, so a
//!   falsy `changedTiles` never reaches it.
//! * `railroadDirty` / `structuresDirty` / `relationsDirty` are truthy gates
//!   over scripted numbers (`NaN` / `0` falsy, everything else truthy).
//! * The three event lists gate on `length > 0` independently.
//! * `updateSpiralRibbons`, `updateUnits`, `updateAttackRings`,
//!   `updateNukeTelegraphs`, `updateNames` (snap literally `false`) and
//!   `setSAMAllianceClusters` are UNCONDITIONAL.

/// The `FrameUploadTarget` method ids (interface declaration order).
pub const M_TILE_TRAIL: f64 = 0.0;
pub const M_LIVE_DELTA: f64 = 1.0;
pub const M_LIVE_TRAIL_DELTA: f64 = 2.0;
pub const M_SPIRAL: f64 = 3.0;
pub const M_RAILROAD: f64 = 4.0;
pub const M_RAIL_DUST: f64 = 5.0;
pub const M_UNITS: f64 = 6.0;
pub const M_STRUCTURES: f64 = 7.0;
pub const M_DEAD: f64 = 8.0;
pub const M_CONQUEST: f64 = 9.0;
pub const M_BONUS: f64 = 10.0;
pub const M_RINGS: f64 = 11.0;
pub const M_TELEGRAPHS: f64 = 12.0;
pub const M_NAMES: f64 = 13.0;
pub const M_RELATIONS: f64 = 14.0;
pub const M_CLUSTERS: f64 = 15.0;

/// JS truthiness over the numeric wire domain (`NaN` / `0` / `-0` falsy).
fn truthy_num(v: f64) -> bool {
    v != 0.0 && !v.is_nan()
}

/// The scripted `FrameData` gates the dispatch reads.
#[derive(Debug, Clone, Copy, Default)]
pub struct FrameGates {
    /// `changedTiles` tag: 0 falsy (null/undefined/""), 1 array.
    pub changed_tag: f64,
    /// `changedTiles.length` (only meaningful when `changed_tag == 1`).
    pub changed_len: f64,
    pub trail_dirty_row_min: f64,
    pub trail_dirty_row_max: f64,
    pub railroad_dirty: f64,
    pub revealed_rail_tiles_len: f64,
    pub tick: f64,
    pub structures_dirty: f64,
    pub dead_units_len: f64,
    pub conquest_events_len: f64,
    pub bonus_events_len: f64,
    pub relations_dirty: f64,
    pub relation_size: f64,
}

/// `uploadFrameData` over the trace recorder — returns the `(methodId,
/// params…)` event stream.
pub fn upload_frame_data(f: &FrameGates, trace: &mut Vec<f64>) {
    trace.clear();
    // --- Tiles + Trails ---
    if f.changed_tag == 1.0 {
        // truthy branch (an array; `[]` included).
        if f.changed_len > 0.0 {
            trace.push(M_LIVE_DELTA);
        }
        if f.trail_dirty_row_max >= 0.0 {
            trace.push(M_LIVE_TRAIL_DELTA);
            trace.push(f.trail_dirty_row_min);
            trace.push(f.trail_dirty_row_max);
        }
    } else {
        trace.push(M_TILE_TRAIL);
    }
    trace.push(M_SPIRAL);

    // --- Railroads ---
    if truthy_num(f.railroad_dirty) {
        trace.push(M_RAILROAD);
        if f.revealed_rail_tiles_len > 0.0 {
            trace.push(M_RAIL_DUST);
        }
    }

    // --- Units + structures ---
    trace.push(M_UNITS);
    trace.push(f.tick);
    if truthy_num(f.structures_dirty) {
        trace.push(M_STRUCTURES);
    }

    // --- Ephemeral effects ---
    if f.dead_units_len > 0.0 {
        trace.push(M_DEAD);
    }
    if f.conquest_events_len > 0.0 {
        trace.push(M_CONQUEST);
    }
    if f.bonus_events_len > 0.0 {
        trace.push(M_BONUS);
    }

    // --- Attack rings + nuke telegraphs ---
    trace.push(M_RINGS);
    trace.push(M_TELEGRAPHS);

    // --- Names + player status (snap literally false) ---
    trace.push(M_NAMES);
    trace.push(0.0);

    // --- Relations ---
    if truthy_num(f.relations_dirty) {
        trace.push(M_RELATIONS);
        trace.push(f.relation_size);
    }

    // --- Alliance clusters (SAM pass) ---
    trace.push(M_CLUSTERS);
}

// ---------------------------------------------------------------- vectors op
//
// kind 0: [changedTag, changedLen, rowMin, rowMax, railroadDirty,
//          revealedLen, tick, structuresDirty, deadLen, conquestLen,
//          bonusLen, relationsDirty, relationSize]  (13 tokens)
//         -> [count, (methodId, params…)*count]  the full dispatch trace.

pub fn run_op(kind: u8, args: &[f64]) -> Vec<f64> {
    let mut out = Vec::new();
    match kind {
        0 => {
            let f = FrameGates {
                changed_tag: args[0],
                changed_len: args[1],
                trail_dirty_row_min: args[2],
                trail_dirty_row_max: args[3],
                railroad_dirty: args[4],
                revealed_rail_tiles_len: args[5],
                tick: args[6],
                structures_dirty: args[7],
                dead_units_len: args[8],
                conquest_events_len: args[9],
                bonus_events_len: args[10],
                relations_dirty: args[11],
                relation_size: args[12],
            };
            let mut trace = Vec::new();
            upload_frame_data(&f, &mut trace);
            // Count the events (method ids, excluding pushed params).
            let mut count = 0.0;
            let mut i = 0usize;
            while i < trace.len() {
                count += 1.0;
                i += match trace[i] as i32 {
                    2 => 3,
                    6 => 2,
                    13 => 2,
                    14 => 2,
                    _ => 1,
                };
            }
            out.push(count);
            out.extend(trace);
        }
        k => unreachable!("frame_upload: unknown op kind {k}"),
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gates() -> FrameGates {
        FrameGates {
            changed_tag: 1.0,
            changed_len: 1.0,
            trail_dirty_row_min: 0.0,
            trail_dirty_row_max: 5.0,
            railroad_dirty: 1.0,
            revealed_rail_tiles_len: 1.0,
            tick: 42.0,
            structures_dirty: 1.0,
            dead_units_len: 1.0,
            conquest_events_len: 1.0,
            bonus_events_len: 1.0,
            relations_dirty: 1.0,
            relation_size: 3.0,
        }
    }

    #[test]
    fn all_gates_on_full_trace() {
        let mut t = Vec::new();
        upload_frame_data(&gates(), &mut t);
        assert_eq!(
            t,
            [
                M_LIVE_DELTA,
                M_LIVE_TRAIL_DELTA,
                0.0,
                5.0,
                M_SPIRAL,
                M_RAILROAD,
                M_RAIL_DUST,
                M_UNITS,
                42.0,
                M_STRUCTURES,
                M_DEAD,
                M_CONQUEST,
                M_BONUS,
                M_RINGS,
                M_TELEGRAPHS,
                M_NAMES,
                0.0,
                M_RELATIONS,
                3.0,
                M_CLUSTERS,
            ]
        );
    }

    #[test]
    fn empty_array_changed_tiles_is_truthy_but_skips_delta() {
        let mut f = gates();
        f.changed_len = 0.0;
        f.trail_dirty_row_max = -1.0;
        let mut t = Vec::new();
        upload_frame_data(&f, &mut t);
        // No delta, no trail delta, and NO full tile/trail upload either.
        assert_eq!(t[0], M_SPIRAL);
    }

    #[test]
    fn falsy_changed_tiles_takes_the_else_branch() {
        let mut f = gates();
        f.changed_tag = 0.0;
        let mut t = Vec::new();
        upload_frame_data(&f, &mut t);
        assert_eq!(t[0], M_TILE_TRAIL);
        assert_eq!(t[1], M_SPIRAL);
    }

    #[test]
    fn trail_dirty_row_max_numeric_gates() {
        let mk = |mx: f64| {
            let mut f = gates();
            f.trail_dirty_row_max = mx;
            let mut t = Vec::new();
            upload_frame_data(&f, &mut t);
            t.contains(&M_LIVE_TRAIL_DELTA)
        };
        assert!(mk(0.0));
        assert!(mk(-0.0)); // -0 >= 0 is true.
        assert!(mk(7.0));
        assert!(!mk(-1.0));
        assert!(!mk(f64::NAN)); // NaN >= 0 is false.
    }

    #[test]
    fn truthy_num_gates_nan_and_zero() {
        assert!(!truthy_num(f64::NAN));
        assert!(!truthy_num(0.0));
        assert!(!truthy_num(-0.0));
        assert!(truthy_num(1.0));
        assert!(truthy_num(-1.0));
        assert!(truthy_num(f64::INFINITY));
    }

    #[test]
    fn run_op_count_matches_walk() {
        let a = [
            1.0, 1.0, 0.0, 5.0, 1.0, 1.0, 42.0, 1.0, 1.0, 1.0, 1.0, 1.0, 3.0,
        ];
        let r = run_op(0, &a);
        assert_eq!(r[0], 15.0); // 15 events in the full-gates trace.
    }
}
