//! Port of the runtime-value subset of `src/core/StatsSchemas.ts`: the three
//! `as const` unit-name arrays, the two UnitType -> short-name lookup tables,
//! the numeric index constants, and the module-private `toBigInt` coercion.
//!
//! Scope: the plain data the stats wire format is built from. Faithfulness
//! notes:
//!
//! * The `z.*` / `zb.*` schema declarations (`BombUnitSchema` …
//!   `ArchivedPlayerStatsSchema`) are wire-validation and are not ported —
//!   the values here never touch them at runtime (the capture loads the
//!   module with inert zod/zb Proxies, same shim as `cosmetic_schemas`).
//! * `unitTypeToBoatUnit` is commented out in the TS and is not ported.
//! * The TS lookup objects are keyed by the `UnitType` string-enum values
//!   ("Atom Bomb", "Missile Silo", …); the tables below keep the TS computed-
//!   key insertion order so the dumps match declaration-for-declaration.
//! * `toBigInt` returns a JS `bigint` (arbitrary precision). The Rust twin
//!   returns `i64`; every parity scenario stays within |v| <= 2^53 so the
//!   value is exactly representable as the f64 token that crosses the wire.
//!   Inputs whose decimal string overflows `i64` (valid TS `BigInt`) map to
//!   `None` here — out of scope for the captured surface.
//! * All comparisons are JS `===` on strings; every string in this module is
//!   ASCII, so Rust `&str` equality matches code-unit-for-code-unit.

/// `bombUnits` — the four nuke short names (`as const` in the TS).
pub const BOMB_UNITS: [&str; 4] = ["abomb", "hbomb", "mirv", "mirvw"];

/// `boatUnits` — the two boat short names (`as const` in the TS).
pub const BOAT_UNITS: [&str; 2] = ["trade", "trans"];

/// `otherUnits` — the seven structure/warship short names (`as const`).
pub const OTHER_UNITS: [&str; 7] = ["city", "defp", "port", "wshp", "silo", "saml", "fact"];

/// `unitTypeToBombUnit` — UnitType value -> bomb short name, in TS
/// computed-key insertion order.
pub const UNIT_TYPE_TO_BOMB_UNIT: [(&str, &str); 4] = [
    ("Atom Bomb", "abomb"),
    ("Hydrogen Bomb", "hbomb"),
    ("MIRV", "mirv"),
    ("MIRV Warhead", "mirvw"),
];

/// `unitTypeToOtherUnit` — UnitType value -> other-unit short name, in TS
/// computed-key insertion order.
pub const UNIT_TYPE_TO_OTHER_UNIT: [(&str, &str); 7] = [
    ("City", "city"),
    ("Defense Post", "defp"),
    ("Missile Silo", "silo"),
    ("Port", "port"),
    ("SAM Launcher", "saml"),
    ("Warship", "wshp"),
    ("Factory", "fact"),
];

/// `unitTypeToBombUnit[UnitType.AtomBomb]` etc. — `None` for an unknown key
/// (the TS object simply lacks the property).
pub fn unit_type_to_bomb_unit(ut: &str) -> Option<&'static str> {
    UNIT_TYPE_TO_BOMB_UNIT
        .iter()
        .find(|(k, _)| *k == ut)
        .map(|(_, v)| *v)
}

/// `unitTypeToOtherUnit[UnitType.City]` etc. — `None` for an unknown key.
pub fn unit_type_to_other_unit(ut: &str) -> Option<&'static str> {
    UNIT_TYPE_TO_OTHER_UNIT
        .iter()
        .find(|(k, _)| *k == ut)
        .map(|(_, v)| *v)
}

// ---------------------------------------------------------------- index consts
//
// The numeric array-slot indices the stats wire layout documents, in TS
// declaration order. `INDEX_CONSTS` is the parity anchor the run_op kind 5
// dump walks; the individual consts are the typed surface callers use.

/// `ATTACK_INDEX_SENT` — outgoing attack troops.
pub const ATTACK_INDEX_SENT: usize = 0;
/// `ATTACK_INDEX_RECV` — incoming attack troops.
pub const ATTACK_INDEX_RECV: usize = 1;
/// `ATTACK_INDEX_CANCEL` — cancelled attack troops.
pub const ATTACK_INDEX_CANCEL: usize = 2;
/// `ATTACK_INDEX_MAX_RECV` — largest single incoming attack (running max).
pub const ATTACK_INDEX_MAX_RECV: usize = 3;

/// `TILE_INDEX_PEAK` — high-water mark of tiles owned.
pub const TILE_INDEX_PEAK: usize = 0;
/// `TILE_INDEX_DRAWDOWN_PEAK` — peak the worst drawdown started from.
pub const TILE_INDEX_DRAWDOWN_PEAK: usize = 1;
/// `TILE_INDEX_DRAWDOWN_TROUGH` — trough of the worst drawdown.
pub const TILE_INDEX_DRAWDOWN_TROUGH: usize = 2;

/// `ALLIANCE_INDEX_FORMED`
pub const ALLIANCE_INDEX_FORMED: usize = 0;
/// `ALLIANCE_INDEX_BROKEN_BY_OTHER` — betrayed side.
pub const ALLIANCE_INDEX_BROKEN_BY_OTHER: usize = 1;
/// `ALLIANCE_INDEX_EXPIRED`
pub const ALLIANCE_INDEX_EXPIRED: usize = 2;
/// `ALLIANCE_INDEX_HELD_TO_END`
pub const ALLIANCE_INDEX_HELD_TO_END: usize = 3;
/// `ALLIANCE_INDEX_PEAK_CONCURRENT`
pub const ALLIANCE_INDEX_PEAK_CONCURRENT: usize = 4;
/// `ALLIANCE_INDEX_LONGEST_HELD` — in ticks.
pub const ALLIANCE_INDEX_LONGEST_HELD: usize = 5;

/// `PLAYER_INDEX_HUMAN`
pub const PLAYER_INDEX_HUMAN: usize = 0;
/// `PLAYER_INDEX_NATION`
pub const PLAYER_INDEX_NATION: usize = 1;
/// `PLAYER_INDEX_BOT`
pub const PLAYER_INDEX_BOT: usize = 2;

/// `BOAT_INDEX_SENT` — boats launched.
pub const BOAT_INDEX_SENT: usize = 0;
/// `BOAT_INDEX_ARRIVE`
pub const BOAT_INDEX_ARRIVE: usize = 1;
/// `BOAT_INDEX_CAPTURE`
pub const BOAT_INDEX_CAPTURE: usize = 2;
/// `BOAT_INDEX_DESTROY`
pub const BOAT_INDEX_DESTROY: usize = 3;

/// `BOMB_INDEX_LAUNCH` — bombs launched.
pub const BOMB_INDEX_LAUNCH: usize = 0;
/// `BOMB_INDEX_LAND`
pub const BOMB_INDEX_LAND: usize = 1;
/// `BOMB_INDEX_INTERCEPT`
pub const BOMB_INDEX_INTERCEPT: usize = 2;

/// `GOLD_INDEX_WORK` — gold earned by workers.
pub const GOLD_INDEX_WORK: usize = 0;
/// `GOLD_INDEX_WAR` — gold earned by conquering players.
pub const GOLD_INDEX_WAR: usize = 1;
/// `GOLD_INDEX_TRADE` — gold earned by trade ships.
pub const GOLD_INDEX_TRADE: usize = 2;
/// `GOLD_INDEX_STEAL` — gold earned by capturing trade ships.
pub const GOLD_INDEX_STEAL: usize = 3;
/// `GOLD_INDEX_TRAIN_SELF` — gold earned by own trains.
pub const GOLD_INDEX_TRAIN_SELF: usize = 4;
/// `GOLD_INDEX_TRAIN_OTHER` — gold earned by other players' trains.
pub const GOLD_INDEX_TRAIN_OTHER: usize = 5;

/// `OTHER_INDEX_BUILT` — structures and warships built.
pub const OTHER_INDEX_BUILT: usize = 0;
/// `OTHER_INDEX_DESTROY`
pub const OTHER_INDEX_DESTROY: usize = 1;
/// `OTHER_INDEX_CAPTURE` — structures captured.
pub const OTHER_INDEX_CAPTURE: usize = 2;
/// `OTHER_INDEX_LOST` — destroyed/captured by others.
pub const OTHER_INDEX_LOST: usize = 3;
/// `OTHER_INDEX_UPGRADE` — structures upgraded.
pub const OTHER_INDEX_UPGRADE: usize = 4;

/// Every numeric index constant above, `(name, value)` in TS declaration
/// order (the run_op kind 5 dump / parity anchor).
pub const INDEX_CONSTS: [(&str, f64); 34] = [
    ("ATTACK_INDEX_SENT", ATTACK_INDEX_SENT as f64),
    ("ATTACK_INDEX_RECV", ATTACK_INDEX_RECV as f64),
    ("ATTACK_INDEX_CANCEL", ATTACK_INDEX_CANCEL as f64),
    ("ATTACK_INDEX_MAX_RECV", ATTACK_INDEX_MAX_RECV as f64),
    ("TILE_INDEX_PEAK", TILE_INDEX_PEAK as f64),
    ("TILE_INDEX_DRAWDOWN_PEAK", TILE_INDEX_DRAWDOWN_PEAK as f64),
    ("TILE_INDEX_DRAWDOWN_TROUGH", TILE_INDEX_DRAWDOWN_TROUGH as f64),
    ("ALLIANCE_INDEX_FORMED", ALLIANCE_INDEX_FORMED as f64),
    ("ALLIANCE_INDEX_BROKEN_BY_OTHER", ALLIANCE_INDEX_BROKEN_BY_OTHER as f64),
    ("ALLIANCE_INDEX_EXPIRED", ALLIANCE_INDEX_EXPIRED as f64),
    ("ALLIANCE_INDEX_HELD_TO_END", ALLIANCE_INDEX_HELD_TO_END as f64),
    ("ALLIANCE_INDEX_PEAK_CONCURRENT", ALLIANCE_INDEX_PEAK_CONCURRENT as f64),
    ("ALLIANCE_INDEX_LONGEST_HELD", ALLIANCE_INDEX_LONGEST_HELD as f64),
    ("PLAYER_INDEX_HUMAN", PLAYER_INDEX_HUMAN as f64),
    ("PLAYER_INDEX_NATION", PLAYER_INDEX_NATION as f64),
    ("PLAYER_INDEX_BOT", PLAYER_INDEX_BOT as f64),
    ("BOAT_INDEX_SENT", BOAT_INDEX_SENT as f64),
    ("BOAT_INDEX_ARRIVE", BOAT_INDEX_ARRIVE as f64),
    ("BOAT_INDEX_CAPTURE", BOAT_INDEX_CAPTURE as f64),
    ("BOAT_INDEX_DESTROY", BOAT_INDEX_DESTROY as f64),
    ("BOMB_INDEX_LAUNCH", BOMB_INDEX_LAUNCH as f64),
    ("BOMB_INDEX_LAND", BOMB_INDEX_LAND as f64),
    ("BOMB_INDEX_INTERCEPT", BOMB_INDEX_INTERCEPT as f64),
    ("GOLD_INDEX_WORK", GOLD_INDEX_WORK as f64),
    ("GOLD_INDEX_WAR", GOLD_INDEX_WAR as f64),
    ("GOLD_INDEX_TRADE", GOLD_INDEX_TRADE as f64),
    ("GOLD_INDEX_STEAL", GOLD_INDEX_STEAL as f64),
    ("GOLD_INDEX_TRAIN_SELF", GOLD_INDEX_TRAIN_SELF as f64),
    ("GOLD_INDEX_TRAIN_OTHER", GOLD_INDEX_TRAIN_OTHER as f64),
    ("OTHER_INDEX_BUILT", OTHER_INDEX_BUILT as f64),
    ("OTHER_INDEX_DESTROY", OTHER_INDEX_DESTROY as f64),
    ("OTHER_INDEX_CAPTURE", OTHER_INDEX_CAPTURE as f64),
    ("OTHER_INDEX_LOST", OTHER_INDEX_LOST as f64),
    ("OTHER_INDEX_UPGRADE", OTHER_INDEX_UPGRADE as f64),
];

// ---------------------------------------------------------------- toBigInt

/// One `toBigInt` input, mirroring the JS runtime shapes the TS branches on.
/// `BigInt` carries the decimal string form (the capture sends values that
/// fit in `i64`; see the module doc-comment).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToBigIntInput<'a> {
    Null,
    Undefined,
    Str(&'a str),
    BigInt(&'a str),
}

/// JS `/^-?\d+$/` over the whole string: optional sign, then one or more
/// ASCII digits, nothing else (empty string fails, " 1" / "+1" / "1.5" fail).
fn is_decimal(s: &str) -> bool {
    let digits = s.strip_prefix('-').unwrap_or(s);
    !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())
}

/// `toBigInt(v)` — bigint passthrough, null/undefined -> 0, decimal strings
/// -> parsed value, anything else throws `ZbEncodeError` in the TS and maps
/// to `None` here. `"-0"` parses to `0`, `"007"` to `7` (matching JS
/// `BigInt`).
pub fn to_bigint(v: ToBigIntInput) -> Option<i64> {
    match v {
        ToBigIntInput::BigInt(s) => s.parse::<i64>().ok(),
        ToBigIntInput::Null | ToBigIntInput::Undefined => Some(0),
        ToBigIntInput::Str(s) => {
            if is_decimal(s) {
                s.parse::<i64>().ok()
            } else {
                None
            }
        }
    }
}

// ---------------------------------------------------------------- vectors op
//
// A flat `f64` token runner shared by the golden replay and the wasm probe.
// Strings cross as `[len, u0, .. u(len-1)]` (UTF-16 code units).
//
// kind table (mirrors the capture):
//   0 [0] -> [4, (str)*4]                       bombUnits dump
//   1 [0] -> [2, (str)*2]                       boatUnits dump
//   2 [0] -> [7, (str)*7]                       otherUnits dump
//   3 [0] -> [4, (key,val)*4]                   unitTypeToBombUnit dump
//   4 [0] -> [7, (key,val)*7]                   unitTypeToOtherUnit dump
//   5 [0] -> [34, (name,value)*34]              INDEX_CONSTS dump (TS order)
//   6 [n,(tag,str?)*n] -> [n,(per-item)*n]      to_bigint batch
//     (tag 0=null, 1=undefined, 2=string, 3=bigint decimal; per-item
//     [0,value] on success, [1] on the throw branch)

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
            out.push(BOMB_UNITS.len() as f64);
            for e in BOMB_UNITS {
                push_string(&mut out, e);
            }
        }
        1 => {
            out.push(BOAT_UNITS.len() as f64);
            for e in BOAT_UNITS {
                push_string(&mut out, e);
            }
        }
        2 => {
            out.push(OTHER_UNITS.len() as f64);
            for e in OTHER_UNITS {
                push_string(&mut out, e);
            }
        }
        3 => {
            out.push(UNIT_TYPE_TO_BOMB_UNIT.len() as f64);
            for (k, v) in UNIT_TYPE_TO_BOMB_UNIT {
                push_string(&mut out, k);
                push_string(&mut out, v);
            }
        }
        4 => {
            out.push(UNIT_TYPE_TO_OTHER_UNIT.len() as f64);
            for (k, v) in UNIT_TYPE_TO_OTHER_UNIT {
                push_string(&mut out, k);
                push_string(&mut out, v);
            }
        }
        5 => {
            out.push(INDEX_CONSTS.len() as f64);
            for (n, v) in INDEX_CONSTS {
                push_string(&mut out, n);
                out.push(v);
            }
        }
        6 => {
            let n = c.u();
            out.push(n as f64);
            for _ in 0..n {
                let tag = c.u();
                let s = if tag >= 2 { c.string() } else { String::new() };
                let input = match tag {
                    0 => ToBigIntInput::Null,
                    1 => ToBigIntInput::Undefined,
                    2 => ToBigIntInput::Str(&s),
                    _ => ToBigIntInput::BigInt(&s),
                };
                match to_bigint(input) {
                    Some(v) => {
                        out.push(0.0);
                        out.push(v as f64);
                    }
                    None => out.push(1.0),
                }
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
    fn unit_name_tables() {
        assert_eq!(BOMB_UNITS, ["abomb", "hbomb", "mirv", "mirvw"]);
        assert_eq!(BOAT_UNITS, ["trade", "trans"]);
        assert_eq!(
            OTHER_UNITS,
            ["city", "defp", "port", "wshp", "silo", "saml", "fact"]
        );
    }

    #[test]
    fn lookup_tables() {
        // Space-bearing UnitType values are the keys.
        assert_eq!(unit_type_to_bomb_unit("Atom Bomb"), Some("abomb"));
        assert_eq!(unit_type_to_bomb_unit("Hydrogen Bomb"), Some("hbomb"));
        assert_eq!(unit_type_to_bomb_unit("MIRV"), Some("mirv"));
        assert_eq!(unit_type_to_bomb_unit("MIRV Warhead"), Some("mirvw"));
        assert_eq!(unit_type_to_bomb_unit("City"), None);
        assert_eq!(unit_type_to_bomb_unit(""), None);
        assert_eq!(unit_type_to_other_unit("City"), Some("city"));
        assert_eq!(unit_type_to_other_unit("Defense Post"), Some("defp"));
        assert_eq!(unit_type_to_other_unit("Missile Silo"), Some("silo"));
        assert_eq!(unit_type_to_other_unit("Port"), Some("port"));
        assert_eq!(unit_type_to_other_unit("SAM Launcher"), Some("saml"));
        assert_eq!(unit_type_to_other_unit("Warship"), Some("wshp"));
        assert_eq!(unit_type_to_other_unit("Factory"), Some("fact"));
        assert_eq!(unit_type_to_other_unit("Atom Bomb"), None);
        assert_eq!(unit_type_to_other_unit("Train"), None);
    }

    #[test]
    fn to_bigint_branches() {
        assert_eq!(to_bigint(ToBigIntInput::BigInt("0")), Some(0));
        assert_eq!(to_bigint(ToBigIntInput::BigInt("-1")), Some(-1));
        assert_eq!(to_bigint(ToBigIntInput::BigInt("9007199254740992")), Some(9007199254740992));
        assert_eq!(to_bigint(ToBigIntInput::Null), Some(0));
        assert_eq!(to_bigint(ToBigIntInput::Undefined), Some(0));
        assert_eq!(to_bigint(ToBigIntInput::Str("123")), Some(123));
        assert_eq!(to_bigint(ToBigIntInput::Str("-456")), Some(-456));
        assert_eq!(to_bigint(ToBigIntInput::Str("007")), Some(7));
        assert_eq!(to_bigint(ToBigIntInput::Str("-0")), Some(0));
        // Regex misses -> the ZbEncodeError throw branch.
        assert_eq!(to_bigint(ToBigIntInput::Str("")), None);
        assert_eq!(to_bigint(ToBigIntInput::Str("1.5")), None);
        assert_eq!(to_bigint(ToBigIntInput::Str("1e3")), None);
        assert_eq!(to_bigint(ToBigIntInput::Str(" 1")), None);
        assert_eq!(to_bigint(ToBigIntInput::Str("+1")), None);
        assert_eq!(to_bigint(ToBigIntInput::Str("a")), None);
    }

    #[test]
    fn run_op_dump_shapes() {
        let b = run_op(0, &[0.0]);
        assert_eq!(b[0], 4.0);
        assert_eq!(b[1], 5.0); // "abomb" length
        let bo = run_op(1, &[0.0]);
        assert_eq!(bo[0], 2.0);
        let o = run_op(2, &[0.0]);
        assert_eq!(o[0], 7.0);
        let bm = run_op(3, &[0.0]);
        assert_eq!(bm[0], 4.0);
        assert_eq!(bm[1], 9.0); // "Atom Bomb" length
        let om = run_op(4, &[0.0]);
        assert_eq!(om[0], 7.0);
        let ic = run_op(5, &[0.0]);
        assert_eq!(ic[0], 34.0);
    }

    fn enc_str(s: &str) -> Vec<f64> {
        let units: Vec<u16> = s.encode_utf16().collect();
        let mut v = vec![units.len() as f64];
        v.extend(units.iter().map(|&u| u as f64));
        v
    }

    #[test]
    fn run_op_to_bigint_batch() {
        let mut args = vec![3.0];
        args.push(2.0);
        args.extend(enc_str("007"));
        args.push(0.0);
        args.push(2.0);
        args.extend(enc_str("1.5"));
        let res = run_op(6, &args);
        assert_eq!(res, vec![3.0, 0.0, 7.0, 0.0, 0.0, 1.0]);
    }
}
