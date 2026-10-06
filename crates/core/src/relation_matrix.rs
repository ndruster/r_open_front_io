//! Port of `src/client/render/frame/derive/RelationMatrix.ts` — the shared
//! 1 MiB relationship matrix behind the nuke-telegraph relation classes.
//!
//! Faithfulness notes (quirk list):
//!
//! * The module-level `matrix` is ONE `Uint8Array(1024*1024)` reused across
//!   frames — the harness holds it and `fill(0)` rewrites it every call (the
//!   buffer identity is observable in JS, not through values).
//! * Teams gate: `teams && teams.size > 0` — an empty Map skips the whole
//!   teammate pass. `byTeam` is a string-keyed Map in insertion order; the
//!   sid gate is `sid <= 0 || sid >= 1024` (continue).
//! * Same-team pairs (i < j) write FRIENDLY BOTH directions unconditionally.
//! * Alliance upgrade is `if (matrix[ab] < 1) matrix[ab] = 1` — an EMBARGO
//!   (2) is NEVER downgraded by a later alliance. Player gate `sid <= 0 ||
//!   sid >= 1024` continues; ally gate `allyID > 0 && allyID < 1024`.
//! * Embargo writes are UNCONDITIONAL overwrites (both directions) — they
//!   replace a friendly, and a same-team friendly written earlier for the
//!   same pair loses.
//! * `buildTeamMap`: `p.team !== null` (STRICT) — `undefined` would pass;
//!   the codec keeps null as the only absent form. Map insertion order is
//!   the player-array order.

use crate::desync_detector::NumMap;
use crate::js_json::{push_str, read_str};
use crate::renderer_consts::{read_player, read_static, PlayerState, PlayerStatic};

/// `RELATION_SIZE`.
pub const RELATION_SIZE: usize = 1024;
/// `RELATION_NEUTRAL`.
pub const RELATION_NEUTRAL: u8 = 0;
/// `RELATION_FRIENDLY`.
pub const RELATION_FRIENDLY: u8 = 1;
/// `RELATION_EMBARGO`.
pub const RELATION_EMBARGO: u8 = 2;

/// The harness: the module-level reusable buffer plus the ops.
#[derive(Debug)]
pub struct RigHarness {
    matrix: Vec<u8>,
}

impl Default for RigHarness {
    fn default() -> Self {
        Self { matrix: vec![RELATION_NEUTRAL; RELATION_SIZE * RELATION_SIZE] }
    }
}

impl RigHarness {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// `buildRelationMatrix(players, teams?)` over the shared buffer.
    pub(crate) fn build(&mut self, players: &NumMap<PlayerState>, teams: Option<&Vec<(f64, String)>>) {
        self.matrix.fill(RELATION_NEUTRAL);

        // Teammates — same-team pairs friendly (before embargoes override).
        if let Some(t) = teams {
            if !t.is_empty() {
                let mut by_team: Vec<(String, Vec<f64>)> = Vec::new();
                for (sid, team) in t {
                    if *sid <= 0.0 || *sid >= RELATION_SIZE as f64 {
                        continue;
                    }
                    match by_team.iter_mut().find(|(k, _)| k == team) {
                        Some((_, bucket)) => bucket.push(*sid),
                        None => by_team.push((team.clone(), vec![*sid])),
                    }
                }
                for members in by_team.iter().map(|(_, v)| v) {
                    for i in 0..members.len() {
                        for j in (i + 1)..members.len() {
                            let a = members[i];
                            let b = members[j];
                            self.matrix[(a * RELATION_SIZE as f64 + b) as usize] =
                                RELATION_FRIENDLY;
                            self.matrix[(b * RELATION_SIZE as f64 + a) as usize] =
                                RELATION_FRIENDLY;
                        }
                    }
                }
            }
        }

        for ps in players.values() {
            let sid = ps.small_id;
            if sid <= 0.0 || sid >= RELATION_SIZE as f64 {
                continue;
            }
            for ally_id in &ps.allies {
                if *ally_id > 0.0 && *ally_id < RELATION_SIZE as f64 {
                    let ab = (sid * RELATION_SIZE as f64 + ally_id) as usize;
                    let ba = (ally_id * RELATION_SIZE as f64 + sid) as usize;
                    if self.matrix[ab] < RELATION_FRIENDLY {
                        self.matrix[ab] = RELATION_FRIENDLY;
                    }
                    if self.matrix[ba] < RELATION_FRIENDLY {
                        self.matrix[ba] = RELATION_FRIENDLY;
                    }
                }
            }
            for e_id in &ps.embargoes {
                if *e_id > 0.0 && *e_id < RELATION_SIZE as f64 {
                    self.matrix[(sid * RELATION_SIZE as f64 + e_id) as usize] = RELATION_EMBARGO;
                    self.matrix[(e_id * RELATION_SIZE as f64 + sid) as usize] = RELATION_EMBARGO;
                }
            }
        }
    }

    /// `buildTeamMap(players)` — the insertion-ordered `(smallID, team)` list.
    pub fn build_team_map(players: &[PlayerStatic]) -> Vec<(f64, String)> {
        let mut m: NumMap<String> = NumMap::default();
        for p in players {
            if let Some(team) = &p.team {
                m.set(p.small_id, team.clone());
            }
        }
        m.iter().map(|(k, v)| (k, v.clone())).collect()
    }

    /// Nonzero-cell dump `[k, (index, value)*k]` in ascending index order —
    /// the 1 MiB buffer is too big to cross whole; the zero fill is pinned
    /// by every absent index being 0 on both sides.
    pub fn dump_nonzero(&self) -> Vec<f64> {
        let cells: Vec<(usize, u8)> = self
            .matrix
            .iter()
            .enumerate()
            .filter(|(_, v)| **v != 0)
            .map(|(i, v)| (i, *v))
            .collect();
        let mut out = Vec::with_capacity(1 + 2 * cells.len());
        out.push(cells.len() as f64);
        for (i, v) in cells {
            out.push(i as f64);
            out.push(v as f64);
        }
        out
    }

    /// Run one op. Kind table (see `tools/gen_vectors.mjs`):
    /// 0 reset -> `[0]`;
    /// 1 build `[playersN, (PlayerState)*n, teamsFlag, teamsN, (sid,
    ///   team-str)*teamsN]` -> nonzero dump `[k, (index, value)*k]`;
    /// 2 buildTeamMap `[n, (PlayerStatic)*n]` -> `[m, (sid, team-str)*m]`;
    /// 3 dump -> nonzero dump (state probe between builds).
    pub fn run_op(&mut self, kind: u8, args: &[f64]) -> Vec<f64> {
        let mut i = 0usize;
        match kind {
            0 => {
                self.reset();
                vec![0.0]
            }
            1 => {
                let np = args[i] as usize;
                i += 1;
                let mut players: NumMap<PlayerState> = NumMap::default();
                for _ in 0..np {
                    let p = read_player(args, &mut i);
                    players.set(p.small_id, p);
                }
                let teams_flag = args[i] != 0.0;
                i += 1;
                let mut teams: Vec<(f64, String)> = Vec::new();
                if teams_flag {
                    let nt = args[i] as usize;
                    i += 1;
                    for _ in 0..nt {
                        let sid = args[i];
                        i += 1;
                        let team = read_str(args, &mut i);
                        teams.push((sid, team));
                    }
                }
                self.build(&players, if teams_flag { Some(&teams) } else { None });
                self.dump_nonzero()
            }
            2 => {
                let n = args[i] as usize;
                i += 1;
                let statics: Vec<PlayerStatic> =
                    (0..n).map(|_| read_static(args, &mut i)).collect();
                let m = Self::build_team_map(&statics);
                let mut out = vec![m.len() as f64];
                for (sid, team) in &m {
                    out.push(*sid);
                    push_str(&mut out, team);
                }
                out
            }
            3 => self.dump_nonzero(),
            k => unreachable!("relation_matrix harness: unknown op kind {k}"),
        }
    }
}
