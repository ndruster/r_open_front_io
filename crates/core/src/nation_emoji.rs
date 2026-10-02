//! Port of `src/core/Util.ts` (`emojiTable` / `flattenedEmojiTable`) plus the
//! 23 `EMOJI_*` id arrays from `src/core/execution/nation/NationEmojiBehavior.ts`.
//!
//! Scope: the emoji picker grid (12 rows x 5), its flattened 60-entry form,
//! and the behaviour constants. `emojiId = (e) => flattenedEmojiTable.indexOf(e)`
//! is the only function the constants depend on; the `NationEmojiBehavior`
//! class and the `respondTo*` functions ride on `Game` / `Player` /
//! `EmojiExecution` and are a later port.
//!
//! Faithfulness notes:
//!
//! * The strings cross the parity wire as UTF-16 code-unit sequences, so the
//!   surrogate pairs (e.g. `😀` = D83D DE00) and the variation selectors /
//!   ZWJ sequences (e.g. `🤦‍♂️` = 1F926 200D 2642 FE0F, `❤️` = 2764 FE0F)
//!   are compared code-unit-exact. Entries that are not a single code point
//!   are written with `\u{..}` escapes to pin every trailing unit.
//! * `flattenedEmojiTable` is `emojiTable.flat()` — row-major order; all 60
//!   entries are distinct, so `indexOf` is a pure first-match lookup and Rust
//!   `&str` equality matches JS string equality here (both compare the
//!   code-unit sequence).
//! * The `EMOJI_*` arrays store the emoji *literals* (the TS source shape);
//!   the numeric ids the TS module exports are derived through `emoji_id` in
//!   the same declaration order.

/// `emojiTable` — the 12x5 picker grid (`as const` in the TS).
pub const EMOJI_TABLE: [&[&str; 5]; 12] = [
    &["\u{1F600}", "\u{1F60A}", "\u{1F970}", "\u{1F607}", "\u{1F60E}"],
    &["\u{1F61E}", "\u{1F97A}", "\u{1F62D}", "\u{1F631}", "\u{1F621}"],
    &["\u{1F608}", "\u{1F921}", "\u{1F971}", "\u{1FAE1}", "\u{1F595}"],
    &["\u{1F44B}", "\u{1F44F}", "\u{270B}", "\u{1F64F}", "\u{1F4AA}"],
    &[
        "\u{1F44D}",
        "\u{1F44E}",
        "\u{1FAF4}",
        "\u{1F90C}",
        "\u{1F926}\u{200D}\u{2642}\u{FE0F}",
    ],
    &[
        "\u{1F91D}",
        "\u{1F198}",
        "\u{1F54A}\u{FE0F}",
        "\u{1F3F3}\u{FE0F}",
        "\u{23F3}",
    ],
    &[
        "\u{1F525}",
        "\u{1F4A5}",
        "\u{1F480}",
        "\u{2622}\u{FE0F}",
        "\u{26A0}\u{FE0F}",
    ],
    &[
        "\u{2196}\u{FE0F}",
        "\u{2B06}\u{FE0F}",
        "\u{2197}\u{FE0F}",
        "\u{1F451}",
        "\u{1F947}",
    ],
    &[
        "\u{2B05}\u{FE0F}",
        "\u{1F3AF}",
        "\u{27A1}\u{FE0F}",
        "\u{1F948}",
        "\u{1F949}",
    ],
    &[
        "\u{2199}\u{FE0F}",
        "\u{2B07}\u{FE0F}",
        "\u{2198}\u{FE0F}",
        "\u{2764}\u{FE0F}",
        "\u{1F494}",
    ],
    &[
        "\u{1F4B0}",
        "\u{2693}",
        "\u{26F5}",
        "\u{1F3E1}",
        "\u{1F6E1}\u{FE0F}",
    ],
    &["\u{1F3ED}", "\u{1F682}", "\u{2753}", "\u{1F414}", "\u{1F400}"],
];

/// `flattenedEmojiTable = emojiTable.flat()` — 60 entries, row-major.
pub const FLATTENED_EMOJI_TABLE: [&str; 60] = [
    "\u{1F600}",
    "\u{1F60A}",
    "\u{1F970}",
    "\u{1F607}",
    "\u{1F60E}",
    "\u{1F61E}",
    "\u{1F97A}",
    "\u{1F62D}",
    "\u{1F631}",
    "\u{1F621}",
    "\u{1F608}",
    "\u{1F921}",
    "\u{1F971}",
    "\u{1FAE1}",
    "\u{1F595}",
    "\u{1F44B}",
    "\u{1F44F}",
    "\u{270B}",
    "\u{1F64F}",
    "\u{1F4AA}",
    "\u{1F44D}",
    "\u{1F44E}",
    "\u{1FAF4}",
    "\u{1F90C}",
    "\u{1F926}\u{200D}\u{2642}\u{FE0F}",
    "\u{1F91D}",
    "\u{1F198}",
    "\u{1F54A}\u{FE0F}",
    "\u{1F3F3}\u{FE0F}",
    "\u{23F3}",
    "\u{1F525}",
    "\u{1F4A5}",
    "\u{1F480}",
    "\u{2622}\u{FE0F}",
    "\u{26A0}\u{FE0F}",
    "\u{2196}\u{FE0F}",
    "\u{2B06}\u{FE0F}",
    "\u{2197}\u{FE0F}",
    "\u{1F451}",
    "\u{1F947}",
    "\u{2B05}\u{FE0F}",
    "\u{1F3AF}",
    "\u{27A1}\u{FE0F}",
    "\u{1F948}",
    "\u{1F949}",
    "\u{2199}\u{FE0F}",
    "\u{2B07}\u{FE0F}",
    "\u{2198}\u{FE0F}",
    "\u{2764}\u{FE0F}",
    "\u{1F494}",
    "\u{1F4B0}",
    "\u{2693}",
    "\u{26F5}",
    "\u{1F3E1}",
    "\u{1F6E1}\u{FE0F}",
    "\u{1F3ED}",
    "\u{1F682}",
    "\u{2753}",
    "\u{1F414}",
    "\u{1F400}",
];

/// `emojiId(e)` — first index in the flattened table, -1 when absent (JS
/// `indexOf` on strings; every entry is distinct so first == only).
pub fn emoji_id(e: &str) -> i32 {
    FLATTENED_EMOJI_TABLE.iter().position(|x| *x == e).map_or(-1, |i| i as i32)
}

/// The 23 `EMOJI_*` constants in TS declaration order: (name, literals).
pub const EMOJI_CONSTS: [(&str, &[&str]); 23] = [
    ("EMOJI_ASSIST_ACCEPT", &["\u{1F44D}", "\u{1F91D}", "\u{1F3AF}"]),
    (
        "EMOJI_ASSIST_RELATION_TOO_LOW",
        &["\u{1F971}", "\u{1F926}\u{200D}\u{2642}\u{FE0F}"],
    ),
    ("EMOJI_ASSIST_TARGET_ME", &["\u{1F97A}", "\u{1F480}"]),
    (
        "EMOJI_ASSIST_TARGET_ALLY",
        &["\u{1F54A}\u{FE0F}", "\u{1F44E}"],
    ),
    ("EMOJI_AGGRESSIVE_ATTACK", &["\u{1F608}"]),
    ("EMOJI_ATTACK", &["\u{1F621}"]),
    ("EMOJI_WARSHIP_RETALIATION", &["\u{26F5}"]),
    ("EMOJI_NUKE", &["\u{2622}\u{FE0F}", "\u{1F4A5}"]),
    (
        "EMOJI_GOT_INSULTED",
        &["\u{1F595}", "\u{1F621}", "\u{1F921}", "\u{1F61E}", "\u{1F62D}"],
    ),
    (
        "EMOJI_LOVE",
        &["\u{2764}\u{FE0F}", "\u{1F60A}", "\u{1F970}"],
    ),
    ("EMOJI_CONFUSED", &["\u{2753}", "\u{1F921}"]),
    ("EMOJI_BRAG", &["\u{1F451}", "\u{1F947}", "\u{1F4AA}"]),
    ("EMOJI_CHARM_ALLIES", &["\u{1F91D}", "\u{1F607}", "\u{1F4AA}"]),
    (
        "EMOJI_CLOWN",
        &["\u{1F921}", "\u{1F926}\u{200D}\u{2642}\u{FE0F}"],
    ),
    ("EMOJI_RAT", &["\u{1F400}"]),
    (
        "EMOJI_OVERWHELMED",
        &[
            "\u{1F480}",
            "\u{1F198}",
            "\u{1F631}",
            "\u{1F97A}",
            "\u{1F62D}",
            "\u{1F61E}",
            "\u{1FAE1}",
            "\u{1F44B}",
        ],
    ),
    ("EMOJI_CONGRATULATE", &["\u{1F44F}"]),
    ("EMOJI_SCARED_OF_THREAT", &["\u{1F64F}", "\u{1F97A}"]),
    ("EMOJI_BORED", &["\u{1F971}"]),
    ("EMOJI_HANDSHAKE", &["\u{1F91D}"]),
    ("EMOJI_DONATION_OK", &["\u{1F44D}"]),
    ("EMOJI_DONATION_TOO_SMALL", &["\u{2753}", "\u{1F971}"]),
    ("EMOJI_GREET", &["\u{1F44B}"]),
];

// ---------------------------------------------------------------- vectors op
//
// A flat `f64` token runner shared by the golden replay and the wasm probe.
// Strings cross as `[len, u0, .. u(len-1)]` (UTF-16 code units).
//
// kind table (mirrors the capture):
//   0 [0] -> [12, (5, (str)*5)*12]              emojiTable dump
//   1 [0] -> [60, (str)*60]                     flattenedEmojiTable dump
//   2 [0] -> [23, (name, len, (id)*len)*23]     EMOJI_* ids in decl order
//   3 [n, (str)*n] -> [n, (id)*n]               emoji_id batch (-1 for absent)

struct Cur<'a>(&'a [f64], usize);
impl<'a> Cur<'a> {
    fn f(&mut self) -> f64 {
        let v = self.0[self.1];
        self.1 += 1;
        v
    }
    fn u(&mut self) -> usize {
        self.f() as usize
    }
    fn string(&mut self) -> String {
        let len = self.u();
        let units: Vec<u16> = (0..len).map(|_| self.f() as u16).collect();
        String::from_utf16_lossy(&units)
    }
}

fn push_string(out: &mut Vec<f64>, s: &str) {
    let units: Vec<u16> = s.encode_utf16().collect();
    out.push(units.len() as f64);
    out.extend(units.iter().map(|&u| u as f64));
}

pub fn run_op(kind: u8, args: &[f64]) -> Vec<f64> {
    let mut out = Vec::new();
    let mut c = Cur(args, 0);
    match kind {
        0 => {
            out.push(EMOJI_TABLE.len() as f64);
            for row in EMOJI_TABLE {
                out.push(row.len() as f64);
                for e in row {
                    push_string(&mut out, e);
                }
            }
        }
        1 => {
            out.push(FLATTENED_EMOJI_TABLE.len() as f64);
            for e in FLATTENED_EMOJI_TABLE {
                push_string(&mut out, e);
            }
        }
        2 => {
            out.push(EMOJI_CONSTS.len() as f64);
            for (name, lits) in EMOJI_CONSTS {
                push_string(&mut out, name);
                out.push(lits.len() as f64);
                for e in lits.iter() {
                    out.push(emoji_id(e) as f64);
                }
            }
        }
        3 => {
            let n = c.u();
            out.push(n as f64);
            for _ in 0..n {
                let s = c.string();
                out.push(emoji_id(&s) as f64);
            }
        }
        _ => {}
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_flattens_row_major() {
        let flat: Vec<&str> = EMOJI_TABLE.iter().flat_map(|r| r.iter().copied()).collect();
        assert_eq!(flat.len(), FLATTENED_EMOJI_TABLE.len());
        assert!(flat.iter().zip(FLATTENED_EMOJI_TABLE.iter()).all(|(a, b)| a == b));
    }

    #[test]
    fn table_entries_are_distinct() {
        let mut sorted = FLATTENED_EMOJI_TABLE.to_vec();
        sorted.sort_unstable();
        let n = sorted.len();
        sorted.dedup();
        assert_eq!(sorted.len(), n);
    }

    #[test]
    fn emoji_id_known_and_unknown() {
        assert_eq!(emoji_id("\u{1F600}"), 0);
        assert_eq!(emoji_id("\u{1F400}"), 59);
        assert_eq!(emoji_id("\u{1F44D}"), 20);
        // ZWJ sequence: 24 is the code-unit-exact match.
        assert_eq!(emoji_id("\u{1F926}\u{200D}\u{2642}\u{FE0F}"), 24);
        // Without the variation selector the code units differ -> -1.
        assert_eq!(emoji_id("\u{1F926}\u{200D}\u{2642}"), -1);
        assert_eq!(emoji_id("\u{2764}"), -1);
        assert_eq!(emoji_id(""), -1);
        assert_eq!(emoji_id("a"), -1);
    }

    #[test]
    fn emoji_consts_resolve_in_table() {
        for (name, lits) in EMOJI_CONSTS {
            for e in lits.iter() {
                assert!(emoji_id(e) >= 0, "{name} has a literal outside the table");
            }
        }
    }

    #[test]
    fn run_op_dump_shapes() {
        let t = run_op(0, &[0.0]);
        assert_eq!(t[0], 12.0);
        // row 0 width 5, then five strings; first string len 2 (surrogate pair).
        assert_eq!(t[1], 5.0);
        assert_eq!(t[2], 2.0);
        let f = run_op(1, &[0.0]);
        assert_eq!(f[0], 60.0);
        let c = run_op(2, &[0.0]);
        assert_eq!(c[0], 23.0);
        // First const: name "EMOJI_ASSIST_ACCEPT" (19 units) + [3, 20, 25, 41].
        assert_eq!(c[1], 19.0);
        let ids = &c[1 + 19 + 1..1 + 19 + 5];
        assert_eq!(ids, &[3.0, 20.0, 25.0, 41.0]);
    }

    #[test]
    fn run_op_id_batch() {
        let mut args = vec![3.0];
        args.extend(emoji_iter(["\u{1F44D}", "\u{1F414}", "\u{1F600}\u{1F600}"]));
        let res = run_op(3, &args);
        assert_eq!(res, vec![3.0, 20.0, 58.0, -1.0]);
    }

    fn emoji_iter<'a>(xs: [&'a str; 3]) -> impl Iterator<Item = f64> + 'a {
        xs.into_iter().flat_map(|s| {
            let units: Vec<u16> = s.encode_utf16().collect();
            std::iter::once(units.len() as f64).chain(units.into_iter().map(|u| u as f64))
        })
    }
}
