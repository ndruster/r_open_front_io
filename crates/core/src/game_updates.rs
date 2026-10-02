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
//   2 [2] -> [n, (key, isArray, len)*n]  createGameUpdatesMap() dump
//   3 [3] -> [n, (0,value | 1,str)*n, m, (kept)*m]  Object.values + filter

/// One runtime value of the enum object: a forward numeric value or a reverse
/// mapping name. The V8 numeric enum carries both, and `Object.values` walks
/// the integer-index (reverse) keys *before* the string (forward) keys — the
/// ECMAScript own-key order (integer keys ascending, then insertion order).
enum GiVal {
    Num(f64),
    Str(&'static str),
}

/// The enum object's `Object.values` table in enumeration order: the 24
/// reverse-mapping names (keys "0".."23") first, then the 24 forward numbers.
fn enum_values() -> Vec<GiVal> {
    let mut v: Vec<GiVal> = NAMES.iter().map(|n| GiVal::Str(n)).collect();
    v.extend((0..NAMES.len()).map(|i| GiVal::Num(i as f64)));
    v
}

/// JS `Number(s)` for the string values the enum table can hold: trimmed
/// whitespace, empty -> `0`, the exact `Infinity` spellings, and a decimal /
/// exponent / hex literal; anything else (every enum name) is `NaN`. The enum
/// object's strings are member names only, so the numeric spellings are
/// defensive.
fn js_number_str(s: &str) -> f64 {
    let t = s.trim();
    if t.is_empty() {
        return 0.0;
    }
    if t == "Infinity" || t == "+Infinity" {
        return f64::INFINITY;
    }
    if t == "-Infinity" {
        return f64::NEG_INFINITY;
    }
    // Only JS-numeric characters may appear; this rejects every enum name
    // (JS would too) before handing the literal to Rust's parser, which
    // otherwise also accepts spellings like "inf" / "NaN" that JS reads as
    // NaN.
    let hex_body = t
        .strip_prefix("0x")
        .or_else(|| t.strip_prefix("0X"))
        .or_else(|| t.strip_prefix("+0x"))
        .or_else(|| t.strip_prefix("+0X"))
        .or_else(|| t.strip_prefix("-0x"))
        .or_else(|| t.strip_prefix("-0X"));
    if let Some(body) = hex_body {
        if body.is_empty() || !body.chars().all(|c| c.is_ascii_hexdigit()) {
            return f64::NAN;
        }
        let mut mag = 0.0f64;
        for c in body.chars() {
            mag = mag * 16.0 + f64::from(c.to_digit(16).unwrap());
        }
        return if t.starts_with('-') { -mag } else { mag };
    }
    let ok = t
        .chars()
        .all(|c| c.is_ascii_digit() || matches!(c, '+' | '-' | '.' | 'e' | 'E'));
    if !ok {
        return f64::NAN;
    }
    t.parse::<f64>().unwrap_or(f64::NAN)
}

/// `!isNaN(Number(key))` on one enum-object value.
fn is_numeric_value(v: &GiVal) -> bool {
    match v {
        GiVal::Num(n) => !n.is_nan(),
        GiVal::Str(s) => !js_number_str(s).is_nan(),
    }
}

/// `createGameUpdatesMap` (`GameImpl.ts` module tail): walk the enum object's
/// values in enumeration order, keep the ones `!isNaN(Number(key))` passes,
/// and write `map[key] = []` — the property name is the JS number→string
/// (`js_num_str`) of the kept value. Returns the insertion-ordered key table
/// with one empty array per key (the array slots are `[isArray=1, len=0]` in
/// the dump).
pub fn create_game_updates_map() -> Vec<Vec<u16>> {
    let mut keys: Vec<Vec<u16>> = Vec::new();
    for v in enum_values() {
        if !is_numeric_value(&v) {
            continue;
        }
        // `map[key as GameUpdateType] = []` — key is a number here, so the
        // property name is Number→string.
        let n = match v {
            GiVal::Num(n) => n,
            GiVal::Str(_) => unreachable!("filtered out above"),
        };
        keys.push(crate::game_ts::js_num_str(n).encode_utf16().collect());
    }
    keys
}

fn run_gi_map(out: &mut Vec<f64>) {
    // Faithful replay of the traverse-filter-write, then dump the result
    // object like the capture: keys in own-key order (all integer-index, so
    // ascending "0".."23"), each value an empty array.
    let keys = create_game_updates_map();
    out.push(keys.len() as f64);
    for k in &keys {
        push_string_units(out, k);
        out.push(1.0); // Array.isArray(map[key])
        out.push(0.0); // map[key].length
    }
}

fn run_gi_values(out: &mut Vec<f64>) {
    let vals = enum_values();
    out.push(vals.len() as f64);
    for v in &vals {
        match v {
            GiVal::Num(n) => {
                out.push(0.0);
                out.push(*n);
            }
            GiVal::Str(s) => {
                out.push(1.0);
                push_string(out, s);
            }
        }
    }
    let kept: Vec<f64> = vals
        .iter()
        .filter(|v| is_numeric_value(v))
        .map(|v| match v {
            GiVal::Num(n) => *n,
            GiVal::Str(_) => unreachable!("not numeric"),
        })
        .collect();
    out.push(kept.len() as f64);
    out.extend(kept.iter().copied());
}

fn push_string_units(out: &mut Vec<f64>, u: &[u16]) {
    out.push(u.len() as f64);
    out.extend(u.iter().map(|&x| f64::from(x)));
}

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
        2 => run_gi_map(&mut out),
        3 => run_gi_values(&mut out),
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
