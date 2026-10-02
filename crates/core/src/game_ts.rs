//! Port of the runtime-value subset of `src/core/game/Game.ts`.
//!
//! Game.ts is mostly interfaces (the `Game` / `Player` / `Unit` facades have
//! no runtime), but it also carries a large block of pure values: twelve
//! enums, the `unitTypeGroup` tables, the message-category map, the `Cell` /
//! `PlayerInfo` classes, and the bulk-cost math. This module ports exactly
//! that subset — the interfaces are type-level only and are not ported.
//!
//! Faithfulness notes:
//!
//! * The string enums are plain `name → value` tables (member order is part
//!   of the dump contract, same as `maps_gen`); the numeric enums carry the
//!   explicit values (`Relation` 0..3, `TerrainType` 0..4, `MessageType`
//!   0..21 — the TS source declares no initializers for `MessageType`, so the
//!   values are the implicit indices).
//! * `isEnumValue` is `Object.values(enumObj).includes(value)` — a strict
//!   equality scan over the *values* only (member names never match).
//! * `unitTypeGroup(types).has(t)` is `Array.prototype.includes`, i.e. strict
//!   equality over the group's own `types` array. `BuildMenus` / `PlayerBuildable`
//!   are built by spreading the earlier groups' `types`, so their member order
//!   is `Structures` then `BuildableAttacks` (then `TransportShip`), not the
//!   `UnitType` declaration order.
//! * `Cell`'s `strRepr` is the template literal `` `Cell[${x},${y}]` `` — JS
//!   `Number`→string, so `-0` prints `"0"` and `1.5` prints `"1.5"`. The
//!   `index` field is declared but never assigned (always `undefined`), so it
//!   is not modelled. [`js_num_str`] covers the domain the capture exercises.
//! * `PlayerInfo.displayName` is `formatPlayerDisplayName(name, clanTag)`,
//!   which is `clanTag ? `[${clanTag}] ${username}` : username` — an *empty*
//!   clan tag is falsy and falls back to the bare name. The ctor defaults
//!   (`isLobbyCreator=false`, `clanTag=null`, `friends=[]`, `teamIndex=null`,
//!   `nationFlag=null`) are echoed as fixed tokens; `clientID` is always
//!   `null` in the capture.
//! * `bulkCost` / `maxBulkAmount` operate on `Gold = bigint`. The capture
//!   keeps every value well below `2^53`, so the port carries them as `f64`
//!   and does the arithmetic in `i64` (exact in that domain). The optional
//!   `upgradeCosts` lookup is `bu.upgradeCosts?.[amount - 1] ?? cost *
//!   BigInt(amount)`: an out-of-range index (including `amount = 0` → index
//!   `-1`) falls back to linear pricing. `maxBulkAmount` breaks *before*
//!   pricing when `n > upgradeCosts.length` (never falls back to linear past
//!   the shipped totals), and the comparison is bigint `>`.
//! * `getMessageCategory` is a plain record lookup; an out-of-domain key
//!   (negative, fractional, ≥ 22) reads `undefined`, which the capture crosses
//!   as the `[0]` miss token.

/// One `name → value` pair of a string enum.
pub struct StrEnumEntry {
    pub name: &'static str,
    pub value: &'static str,
}

/// The nine string enums, indexed by the capture's `enumIdx` (kind 0).
pub const STRING_ENUMS: [&[StrEnumEntry]; 9] = [
    &[
        StrEnumEntry { name: "Easy", value: "Easy" },
        StrEnumEntry { name: "Medium", value: "Medium" },
        StrEnumEntry { name: "Hard", value: "Hard" },
        StrEnumEntry { name: "Impossible", value: "Impossible" },
    ],
    &[
        StrEnumEntry { name: "Singleplayer", value: "Singleplayer" },
        StrEnumEntry { name: "Public", value: "Public" },
        StrEnumEntry { name: "Private", value: "Private" },
    ],
    &[
        StrEnumEntry { name: "FFA", value: "Free For All" },
        StrEnumEntry { name: "Team", value: "Team" },
    ],
    &[
        StrEnumEntry { name: "OneVOne", value: "1v1" },
        StrEnumEntry { name: "TwoVTwo", value: "2v2" },
    ],
    &[
        StrEnumEntry { name: "Compact", value: "Compact" },
        StrEnumEntry { name: "Normal", value: "Normal" },
    ],
    &[
        StrEnumEntry { name: "TransportShip", value: "Transport" },
        StrEnumEntry { name: "Warship", value: "Warship" },
        StrEnumEntry { name: "Shell", value: "Shell" },
        StrEnumEntry { name: "SAMMissile", value: "SAMMissile" },
        StrEnumEntry { name: "Port", value: "Port" },
        StrEnumEntry { name: "AtomBomb", value: "Atom Bomb" },
        StrEnumEntry { name: "HydrogenBomb", value: "Hydrogen Bomb" },
        StrEnumEntry { name: "TradeShip", value: "Trade Ship" },
        StrEnumEntry { name: "MissileSilo", value: "Missile Silo" },
        StrEnumEntry { name: "DefensePost", value: "Defense Post" },
        StrEnumEntry { name: "SAMLauncher", value: "SAM Launcher" },
        StrEnumEntry { name: "City", value: "City" },
        StrEnumEntry { name: "MIRV", value: "MIRV" },
        StrEnumEntry { name: "MIRVWarhead", value: "MIRV Warhead" },
        StrEnumEntry { name: "Train", value: "Train" },
        StrEnumEntry { name: "Factory", value: "Factory" },
    ],
    &[
        StrEnumEntry { name: "Engine", value: "Engine" },
        StrEnumEntry { name: "TailEngine", value: "TailEngine" },
        StrEnumEntry { name: "Carriage", value: "Carriage" },
    ],
    &[
        StrEnumEntry { name: "Bot", value: "BOT" },
        StrEnumEntry { name: "Human", value: "HUMAN" },
        StrEnumEntry { name: "Nation", value: "NATION" },
    ],
    &[
        StrEnumEntry { name: "ATTACK", value: "ATTACK" },
        StrEnumEntry { name: "NUKE", value: "NUKE" },
        StrEnumEntry { name: "ALLIANCE", value: "ALLIANCE" },
        StrEnumEntry { name: "TRADE", value: "TRADE" },
        StrEnumEntry { name: "CHAT", value: "CHAT" },
    ],
];

/// One `name → value` pair of a numeric enum.
pub struct NumEnumEntry {
    pub name: &'static str,
    pub value: f64,
}

/// The three numeric enums, indexed by the capture's `enumIdx` (kind 1).
pub const NUMERIC_ENUMS: [&[NumEnumEntry]; 3] = [
    &[
        NumEnumEntry { name: "Hostile", value: 0.0 },
        NumEnumEntry { name: "Distrustful", value: 1.0 },
        NumEnumEntry { name: "Neutral", value: 2.0 },
        NumEnumEntry { name: "Friendly", value: 3.0 },
    ],
    &[
        NumEnumEntry { name: "Plains", value: 0.0 },
        NumEnumEntry { name: "Highland", value: 1.0 },
        NumEnumEntry { name: "Mountain", value: 2.0 },
        NumEnumEntry { name: "Ocean", value: 3.0 },
        NumEnumEntry { name: "Impassable", value: 4.0 },
    ],
    &[
        NumEnumEntry { name: "ATTACK_FAILED", value: 0.0 },
        NumEnumEntry { name: "ATTACK_CANCELLED", value: 1.0 },
        NumEnumEntry { name: "ATTACK_REQUEST", value: 2.0 },
        NumEnumEntry { name: "CONQUERED_PLAYER", value: 3.0 },
        NumEnumEntry { name: "MIRV_INBOUND", value: 4.0 },
        NumEnumEntry { name: "NUKE_INBOUND", value: 5.0 },
        NumEnumEntry { name: "NUKE_DETONATED", value: 6.0 },
        NumEnumEntry { name: "HYDROGEN_BOMB_INBOUND", value: 7.0 },
        NumEnumEntry { name: "NAVAL_INVASION_INBOUND", value: 8.0 },
        NumEnumEntry { name: "SAM_MISS", value: 9.0 },
        NumEnumEntry { name: "SAM_HIT", value: 10.0 },
        NumEnumEntry { name: "CAPTURED_ENEMY_UNIT", value: 11.0 },
        NumEnumEntry { name: "UNIT_DESTROYED", value: 12.0 },
        NumEnumEntry { name: "ALLIANCE_ACCEPTED", value: 13.0 },
        NumEnumEntry { name: "ALLIANCE_REJECTED", value: 14.0 },
        NumEnumEntry { name: "ALLIANCE_REQUEST", value: 15.0 },
        NumEnumEntry { name: "ALLIANCE_BROKEN", value: 16.0 },
        NumEnumEntry { name: "ALLIANCE_EXPIRED", value: 17.0 },
        NumEnumEntry { name: "DONATION_SENT", value: 18.0 },
        NumEnumEntry { name: "DONATION_RECEIVED", value: 19.0 },
        NumEnumEntry { name: "CHAT", value: 20.0 },
        NumEnumEntry { name: "RENEW_ALLIANCE", value: 21.0 },
    ],
];

/// The 16 `UnitType` values in declaration order — the `has()` matrix columns.
pub const UNIT_TYPE_VALUES: [&str; 16] = [
    "Transport",
    "Warship",
    "Shell",
    "SAMMissile",
    "Port",
    "Atom Bomb",
    "Hydrogen Bomb",
    "Trade Ship",
    "Missile Silo",
    "Defense Post",
    "SAM Launcher",
    "City",
    "MIRV",
    "MIRV Warhead",
    "Train",
    "Factory",
];

/// The five `unitTypeGroup` tables, indexed by the capture's `groupIdx`
/// (kind 2). `BuildMenus` = Structures then BuildableAttacks; `PlayerBuildable`
/// = BuildMenus + TransportShip (spread order, not declaration order).
pub const UNIT_GROUPS: [&[&str]; 5] = [
    &["Atom Bomb", "Hydrogen Bomb", "MIRV Warhead", "MIRV"],
    &["Atom Bomb", "Hydrogen Bomb", "MIRV", "Warship"],
    &["City", "Defense Post", "SAM Launcher", "Missile Silo", "Port", "Factory"],
    &[
        "City",
        "Defense Post",
        "SAM Launcher",
        "Missile Silo",
        "Port",
        "Factory",
        "Atom Bomb",
        "Hydrogen Bomb",
        "MIRV",
        "Warship",
    ],
    &[
        "City",
        "Defense Post",
        "SAM Launcher",
        "Missile Silo",
        "Port",
        "Factory",
        "Atom Bomb",
        "Hydrogen Bomb",
        "MIRV",
        "Warship",
        "Transport",
    ],
];

/// `ColoredTeams`: the 10 entries in key order (kind 5).
pub const COLORED_TEAMS: [(&str, &str); 10] = [
    ("Red", "Red"),
    ("Blue", "Blue"),
    ("Teal", "Teal"),
    ("Purple", "Purple"),
    ("Yellow", "Yellow"),
    ("Orange", "Orange"),
    ("Green", "Green"),
    ("Bot", "Bot"),
    ("Humans", "Humans"),
    ("Nations", "Nations"),
];

/// `MESSAGE_TYPE_CATEGORIES`: category value per `MessageType` key 0..21.
pub const MESSAGE_TYPE_CATEGORIES: [&str; 22] = [
    "ATTACK", "ATTACK", "ATTACK", "ATTACK", "NUKE", "NUKE", "NUKE", "NUKE",
    "ATTACK", "ATTACK", "ATTACK", "ATTACK", "ATTACK", "ALLIANCE", "ALLIANCE",
    "ALLIANCE", "ALLIANCE", "ALLIANCE", "TRADE", "TRADE", "CHAT", "ALLIANCE",
];

/// `isEnumValue(enumObj, value)` — `Object.values(...).includes(value)`.
pub fn is_enum_value(enum_idx: usize, value: &str) -> bool {
    STRING_ENUMS[enum_idx].iter().any(|e| e.value == value)
}

/// `isDifficulty` / `isGameType` / `isGameMode` through the shared guard.
pub fn is_difficulty(value: &str) -> bool {
    is_enum_value(0, value)
}
pub fn is_game_type(value: &str) -> bool {
    is_enum_value(1, value)
}
pub fn is_game_mode(value: &str) -> bool {
    is_enum_value(2, value)
}

/// `getMessageCategory(messageType)` — the record lookup; `None` models the
/// JS `undefined` an out-of-domain key reads.
pub fn get_message_category(message_type: f64) -> Option<&'static str> {
    if message_type.is_finite()
        && (0.0..22.0).contains(&message_type)
        && message_type == message_type.trunc()
    {
        Some(MESSAGE_TYPE_CATEGORIES[message_type as usize])
    } else {
        None
    }
}

/// JS `Number`→string restricted to the domain `Cell`'s template literal
/// exercises: `NaN`, `±Infinity`, or a value JS prints as a plain decimal
/// (integers below `1e21`, `-0` as `"0"`). Fractional inputs use Rust's
/// shortest round-trip `{}`, which agrees with JS for the exactly-representable
/// values the capture uses (`1.5`, `2.25`).
pub(crate) fn js_num_str(v: f64) -> String {
    if v.is_nan() {
        "NaN".to_string()
    } else if v == f64::INFINITY {
        "Infinity".to_string()
    } else if v == f64::NEG_INFINITY {
        "-Infinity".to_string()
    } else if v == v.trunc() && v.abs() < 1e21 {
        // JS prints -0 as "0"; `v as i64` of -0.0 is 0, so format through i64.
        format!("{}", v as i64)
    } else {
        format!("{}", v)
    }
}

/// `Cell`'s `strRepr` — `` `Cell[${x},${y}]` ``.
pub fn cell_str_repr(x: f64, y: f64) -> String {
    format!("Cell[{},{}]", js_num_str(x), js_num_str(y))
}

/// `formatPlayerDisplayName(username, clanTag)` — `clanTag` truthy (present
/// and non-empty) prepends the bracketed tag.
pub fn format_player_display_name(username: &str, clan_tag: Option<&str>) -> String {
    match clan_tag {
        Some(t) if !t.is_empty() => format!("[{}] {}", t, username),
        _ => username.to_string(),
    }
}

/// `bulkCost(bu, amount)`. `upgrade_costs: None` models the absent field; the
/// out-of-range index (including `amount = 0` → `-1`) falls back to linear
/// pricing, same as JS `?.[i] ?? …`. Values stay below `2^53` (exact in i64).
pub fn bulk_cost(cost: i64, upgrade_costs: Option<&[i64]>, amount: i64) -> i64 {
    let idx = amount - 1;
    if let Some(uc) = upgrade_costs {
        if idx >= 0 && (idx as usize) < uc.len() {
            return uc[idx as usize];
        }
    }
    cost * amount
}

/// `maxBulkAmount(bu, gold)` — the loop with the `n > length` break before
/// pricing and the bigint `>` comparison.
pub fn max_bulk_amount(cost: i64, upgrade_costs: Option<&[i64]>, gold: i64) -> i64 {
    let mut max = 0i64;
    for n in 1..=50i64 {
        if let Some(uc) = upgrade_costs {
            if n > uc.len() as i64 {
                break;
            }
        }
        if bulk_cost(cost, upgrade_costs, n) > gold {
            break;
        }
        max = n;
    }
    max
}

// ---------------------------------------------------------------- vectors op
//
// A flat `f64` token runner shared by the golden replay and the wasm probe.
// Strings cross as `[len, u0, .. u(len-1)]` (UTF-16 code units).
//
// kind table (mirrors the capture):
//   0 [enumIdx] -> [n, (name,val)*]
//   1 [enumIdx] -> [n, (name,val)*]
//   2 [groupIdx] -> [tLen,(typeVal)*,16,(has)*]
//   3 [guardIdx, valueStr] -> [0/1]
//   4 [0] -> [22,(cat)*] | [1, mt] -> [0] / [1,(cat)]
//   5 [0] -> [n, (name,val)*]
//   6 [x, y] -> [strRepr, x, y]
//   7 [name,pt,id,clanP,(clan)?] -> [disp,pt,id,lobby,fLen,tiP,nfP,ciP]
//   8 [cost,ucP,(n,(uc)*)?,amount,gold] -> [bulkCost,maxAmount]
//   9 [0] -> [AP,D,T,Q,HVN,MAXU,(2,2,5),(2,5,10)]

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
    fn i(&mut self) -> i64 {
        self.f() as i64
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
            let table = STRING_ENUMS[c.u()];
            out.push(table.len() as f64);
            for e in table {
                push_string(&mut out, e.name);
                push_string(&mut out, e.value);
            }
        }
        1 => {
            let table = NUMERIC_ENUMS[c.u()];
            out.push(table.len() as f64);
            for e in table {
                push_string(&mut out, e.name);
                out.push(e.value);
            }
        }
        2 => {
            let group = UNIT_GROUPS[c.u()];
            out.push(group.len() as f64);
            for t in group {
                push_string(&mut out, t);
            }
            out.push(UNIT_TYPE_VALUES.len() as f64);
            for u in UNIT_TYPE_VALUES {
                out.push(if group.contains(&u) { 1.0 } else { 0.0 });
            }
        }
        3 => {
            let gi = c.u();
            let value = c.string();
            out.push(if is_enum_value(gi, &value) { 1.0 } else { 0.0 });
        }
        4 => {
            if c.u() == 0 {
                out.push(MESSAGE_TYPE_CATEGORIES.len() as f64);
                for cat in MESSAGE_TYPE_CATEGORIES {
                    push_string(&mut out, cat);
                }
            } else {
                let mt = c.f();
                match get_message_category(mt) {
                    Some(cat) => {
                        out.push(1.0);
                        push_string(&mut out, cat);
                    }
                    None => out.push(0.0),
                }
            }
        }
        5 => {
            out.push(COLORED_TEAMS.len() as f64);
            for (k, v) in COLORED_TEAMS {
                push_string(&mut out, k);
                push_string(&mut out, v);
            }
        }
        6 => {
            let x = c.f();
            let y = c.f();
            push_string(&mut out, &cell_str_repr(x, y));
            out.push(x);
            out.push(y);
        }
        7 => {
            let name = c.string();
            let pt = c.string();
            let id = c.string();
            let clan_present = c.u();
            let clan = if clan_present == 1 { Some(c.string()) } else { None };
            let display = format_player_display_name(&name, clan.as_deref());
            push_string(&mut out, &display);
            push_string(&mut out, &pt);
            push_string(&mut out, &id);
            // Fixed ctor defaults: isLobbyCreator false, friends empty,
            // teamIndex / nationFlag / clientID null.
            out.push(0.0);
            out.push(0.0);
            out.push(0.0);
            out.push(0.0);
            out.push(0.0);
        }
        8 => {
            let cost = c.i();
            let uc_present = c.u();
            let uc: Vec<i64> = if uc_present == 1 {
                (0..c.u()).map(|_| c.i()).collect()
            } else {
                Vec::new()
            };
            let amount = c.i();
            let gold = c.i();
            let uc_opt = if uc_present == 1 { Some(uc.as_slice()) } else { None };
            out.push(bulk_cost(cost, uc_opt, amount) as f64);
            out.push(max_bulk_amount(cost, uc_opt, gold) as f64);
        }
        9 => {
            push_string(&mut out, "AllPlayers");
            push_string(&mut out, "Duos");
            push_string(&mut out, "Trios");
            push_string(&mut out, "Quads");
            push_string(&mut out, "Humans Vs Nations");
            out.push(50.0);
            out.push(2.0);
            out.push(2.0);
            out.push(5.0);
            out.push(2.0);
            out.push(5.0);
            out.push(10.0);
        }
        _ => {}
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn string_enum_tables_match_declaration_order() {
        assert_eq!(STRING_ENUMS[0].len(), 4);
        assert_eq!(STRING_ENUMS[5][0].value, "Transport");
        assert_eq!(STRING_ENUMS[5][15].value, "Factory");
        assert_eq!(STRING_ENUMS[7].len(), 3);
        assert_eq!(STRING_ENUMS[8][2].value, "ALLIANCE");
    }

    #[test]
    fn numeric_enums() {
        assert_eq!(NUMERIC_ENUMS[0][3].value, 3.0);
        assert_eq!(NUMERIC_ENUMS[1][4].name, "Impassable");
        assert_eq!(NUMERIC_ENUMS[2].len(), 22);
        assert_eq!(NUMERIC_ENUMS[2][21].value, 21.0);
    }

    #[test]
    fn guards_scan_values_only() {
        assert!(is_difficulty("Easy"));
        assert!(!is_difficulty("easy"));
        // A member *name* that is not a value never matches.
        assert!(!is_game_mode("FFA"));
        assert!(is_game_mode("Free For All"));
        assert!(is_game_type("Singleplayer"));
    }

    #[test]
    fn group_spread_order() {
        // BuildMenus: Structures first, then BuildableAttacks.
        assert_eq!(UNIT_GROUPS[3][0], "City");
        assert_eq!(UNIT_GROUPS[3][6], "Atom Bomb");
        assert_eq!(UNIT_GROUPS[3].len(), 10);
        assert_eq!(UNIT_GROUPS[4][10], "Transport");
    }

    #[test]
    fn message_category_lookup_domain() {
        assert_eq!(get_message_category(0.0), Some("ATTACK"));
        assert_eq!(get_message_category(21.0), Some("ALLIANCE"));
        assert_eq!(get_message_category(22.0), None);
        assert_eq!(get_message_category(-1.0), None);
        assert_eq!(get_message_category(1.5), None);
        assert_eq!(get_message_category(f64::NAN), None);
    }

    #[test]
    fn cell_str_repr_js_number_format() {
        assert_eq!(cell_str_repr(3.0, -4.0), "Cell[3,-4]");
        assert_eq!(cell_str_repr(-0.0, 1.5), "Cell[0,1.5]");
        assert_eq!(cell_str_repr(1.5, 2.25), "Cell[1.5,2.25]");
    }

    #[test]
    fn display_name_clan_truthiness() {
        assert_eq!(format_player_display_name("Bob", None), "Bob");
        assert_eq!(format_player_display_name("Bob", Some("")), "Bob");
        assert_eq!(format_player_display_name("Bob", Some("xyz")), "[xyz] Bob");
    }

    #[test]
    fn bulk_cost_fallbacks() {
        let uc = [10i64, 30, 60];
        assert_eq!(bulk_cost(100, Some(&uc), 2), 30);
        // Out of range -> linear.
        assert_eq!(bulk_cost(100, Some(&uc), 5), 500);
        // amount 0 -> index -1 -> linear 0.
        assert_eq!(bulk_cost(100, Some(&uc), 0), 0);
        assert_eq!(bulk_cost(7, None, 1), 7);
    }

    #[test]
    fn max_bulk_amount_breaks_before_pricing() {
        let uc = [10i64, 30, 60];
        assert_eq!(max_bulk_amount(100, Some(&uc), 35), 2);
        // gold beyond every shipped total still caps at the array length.
        assert_eq!(max_bulk_amount(100, Some(&uc), 1000), 3);
        // No upgradeCosts: linear pricing up to MAX_UPGRADE_AMOUNT.
        assert_eq!(max_bulk_amount(1, None, 100), 50);
        assert_eq!(max_bulk_amount(7, None, 0), 0);
    }

    #[test]
    fn run_op_roundtrip() {
        // kind 6 through the runner.
        let res = run_op(6, &[3.0, -4.0]);
        assert_eq!(res[0], 10.0);
        let s: String = (0..10).map(|i| res[1 + i] as u16 as u8 as char).collect();
        assert_eq!(s, "Cell[3,-4]");
        assert_eq!(res[11], 3.0);
        assert_eq!(res[12], -4.0);
        // kind 9 consts dump: 5 strings + 7 numbers.
        let res9 = run_op(9, &[0.0]);
        assert_eq!(res9[res9.len() - 1], 10.0);
    }
}
