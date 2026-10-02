//! Port of `src/core/game/GameUpdates.ts` (the `GameUpdateType` enum).
//!
//! Scope: the 24-member numeric enum — declaration order is the value
//! (`Tile = 0 .. DonateEvent = 23`). Everything else in the file is
//! `interface` / `type` (wire shapes with no runtime), so it is not ported.
//!
//! Faithfulness notes:
//!
//! * The TS enum has no initializers, so each member takes the previous
//!   value + 1 starting at 0; the port spells the discriminants out.
//! * The wire protocol stamps `type:` with the raw number, so `as_i32` /
//!   `from_i32` are the only conversions that must match the TS numbering.
//! * Name lookup mirrors `GameUpdateType[name]` on the prepared plain-object
//!   form: an absent key yields `undefined`, which the capture maps to `-1`.

/// `GameUpdateType` — the update-kind tag on every game update record.
#[repr(i32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameUpdateType {
    Tile = 0,
    Unit = 1,
    Player = 2,
    DisplayEvent = 3,
    DisplayChatEvent = 4,
    AllianceRequest = 5,
    AllianceRequestReply = 6,
    BrokeAlliance = 7,
    AllianceExpired = 8,
    AllianceExtension = 9,
    TargetPlayer = 10,
    Emoji = 11,
    Win = 12,
    Hash = 13,
    UnitIncoming = 14,
    BonusEvent = 15,
    RailroadDestructionEvent = 16,
    RailroadConstructionEvent = 17,
    RailroadSnapEvent = 18,
    ConquestEvent = 19,
    EmbargoEvent = 20,
    SpawnPhaseEnd = 21,
    GamePaused = 22,
    DonateEvent = 23,
}

/// Member names in declaration order (index == discriminant value).
pub const NAMES: [&str; 24] = [
    "Tile",
    "Unit",
    "Player",
    "DisplayEvent",
    "DisplayChatEvent",
    "AllianceRequest",
    "AllianceRequestReply",
    "BrokeAlliance",
    "AllianceExpired",
    "AllianceExtension",
    "TargetPlayer",
    "Emoji",
    "Win",
    "Hash",
    "UnitIncoming",
    "BonusEvent",
    "RailroadDestructionEvent",
    "RailroadConstructionEvent",
    "RailroadSnapEvent",
    "ConquestEvent",
    "EmbargoEvent",
    "SpawnPhaseEnd",
    "GamePaused",
    "DonateEvent",
];

impl GameUpdateType {
    /// `GameUpdateType[i32]` reverse mapping: `None` outside 0..=23 (the TS
    /// enum has no reverse entry for those numbers).
    pub fn from_i32(v: i32) -> Option<GameUpdateType> {
        match v {
            0 => Some(GameUpdateType::Tile),
            1 => Some(GameUpdateType::Unit),
            2 => Some(GameUpdateType::Player),
            3 => Some(GameUpdateType::DisplayEvent),
            4 => Some(GameUpdateType::DisplayChatEvent),
            5 => Some(GameUpdateType::AllianceRequest),
            6 => Some(GameUpdateType::AllianceRequestReply),
            7 => Some(GameUpdateType::BrokeAlliance),
            8 => Some(GameUpdateType::AllianceExpired),
            9 => Some(GameUpdateType::AllianceExtension),
            10 => Some(GameUpdateType::TargetPlayer),
            11 => Some(GameUpdateType::Emoji),
            12 => Some(GameUpdateType::Win),
            13 => Some(GameUpdateType::Hash),
            14 => Some(GameUpdateType::UnitIncoming),
            15 => Some(GameUpdateType::BonusEvent),
            16 => Some(GameUpdateType::RailroadDestructionEvent),
            17 => Some(GameUpdateType::RailroadConstructionEvent),
            18 => Some(GameUpdateType::RailroadSnapEvent),
            19 => Some(GameUpdateType::ConquestEvent),
            20 => Some(GameUpdateType::EmbargoEvent),
            21 => Some(GameUpdateType::SpawnPhaseEnd),
            22 => Some(GameUpdateType::GamePaused),
            23 => Some(GameUpdateType::DonateEvent),
            _ => None,
        }
    }

    /// The wire value the TS enum member evaluates to.
    pub fn as_i32(self) -> i32 {
        self as i32
    }

    /// The declaration name (TS `GameUpdateType[value]`).
    pub fn name(self) -> &'static str {
        NAMES[self as usize]
    }
}

// ---------------------------------------------------------------- vectors op
//
// A flat `f64` token runner shared by the golden replay and the wasm probe.
// Strings cross as `[len, u0, .. u(len-1)]` (UTF-16 code units).
//
// kind table (mirrors the capture):
//   0 [0] -> [24, (name, value)*24]      full enum dump
//   1 [name] -> [value | -1]             name lookup

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
            out.push(NAMES.len() as f64);
            for (i, n) in NAMES.iter().enumerate() {
                push_string(&mut out, n);
                out.push(i as f64);
            }
        }
        1 => {
            let name = c.string();
            let v = NAMES.iter().position(|n| *n == name).unwrap_or(usize::MAX);
            out.push(if v == usize::MAX { -1.0 } else { v as f64 });
        }
        _ => {}
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discriminants_follow_declaration_order() {
        assert_eq!(GameUpdateType::Tile.as_i32(), 0);
        assert_eq!(GameUpdateType::Emoji.as_i32(), 11);
        assert_eq!(GameUpdateType::RailroadDestructionEvent.as_i32(), 16);
        assert_eq!(GameUpdateType::RailroadConstructionEvent.as_i32(), 17);
        assert_eq!(GameUpdateType::DonateEvent.as_i32(), 23);
    }

    #[test]
    fn names_match_values() {
        assert_eq!(NAMES.len(), 24);
        for (i, n) in NAMES.iter().enumerate() {
            let v = i as i32;
            assert_eq!(GameUpdateType::from_i32(v).unwrap().name(), *n);
            assert_eq!(GameUpdateType::from_i32(v).unwrap().as_i32(), v);
        }
    }

    #[test]
    fn from_i32_out_of_range() {
        assert!(GameUpdateType::from_i32(-1).is_none());
        assert!(GameUpdateType::from_i32(24).is_none());
        assert!(GameUpdateType::from_i32(i32::MAX).is_none());
    }

    #[test]
    fn run_op_dump_roundtrip() {
        let res = run_op(0, &[0.0]);
        assert_eq!(res[0] as usize, 24);
        // Tile is the first pair: name tokenised + value 0.
        assert_eq!(res[1] as usize, 4);
        assert_eq!(&res[2..6], &[84.0, 105.0, 108.0, 101.0]);
        assert_eq!(res[6], 0.0);
        // Last pair is DonateEvent = 23.
        let last = res.len() - 1;
        assert_eq!(res[last], 23.0);
    }

    #[test]
    fn run_op_lookup() {
        let mut args = vec![5.0];
        args.extend("Emoji".encode_utf16().map(|u| u as f64));
        assert_eq!(run_op(1, &args), &[11.0]);
        let mut miss = vec![3.0];
        miss.extend("emo".encode_utf16().map(|u| u as f64));
        assert_eq!(run_op(1, &miss), &[-1.0]);
    }
}
