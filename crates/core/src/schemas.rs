//! Port of the runtime-value subset of `src/core/Schemas.ts`: the enum option
//! arrays (`z.enum(...).options`), the lobby numeric/string constants, the
//! `LogSeverity` string-enum table, the JSON-derived QuickChat key list, and
//! the three regex-backed predicates (`isValidGameID`, the renderable-name
//! character test, and the has-alnum search).
//!
//! Scope: the plain data and pure predicates the wire layer is built from.
//! Faithfulness notes:
//!
//! * The `z.*` / `zb.*` schema declarations (`GameConfigSchema` …
//!   `AnalyticsRecordSchema`) are wire-validation and are not ported — only
//!   their closed-set option arrays are (the capture reads `.options` off a
//!   functional zod shim; everything else rides an inert Proxy).
//! * `QUICK_CHAT_KEYS` mirrors the TS derivation
//!   `Object.entries(QuickChat.json).flatMap(([cat, es]) => es.map(e => `${cat}.${e.key}`))`
//!   — the hard-coded list below is the JSON insertion order (six categories,
//!   58 keys) pinned by the parity dump.
//! * `RENDERABLE_NAME_ALNUM` / `RENDERABLE_NAME_CHARS` are *regex source*
//!   strings: the `\u00C0`-style escapes are literal backslash-u-4hex text,
//!   not code points (the TS source spells them with doubled backslashes).
//! * `RENDERABLE_NAME_CHAR_RE` is `^[<CHARS>]$` with the `u` flag. Node
//!   probing settled the `\\-` ambiguity in the class source: the backslash
//!   only escapes the hyphen, so `-` is a member but `\` itself is NOT, and
//!   there is NO U+005C–U+0061 range (`` ` `` `[` `]` `^` `\` all test
//!   false, `-` tests true). The single-code-point set is therefore
//!   {space, `_`, `.`, `-`} ∪ ASCII alnum ∪ [C0,D6] ∪ [D8,F6] ∪ [F8,FF], and
//!   the `u`-flag anchors iterate by code point: the empty string, a
//!   2-code-point string, and a lone surrogate that is not in the set all
//!   test false (a lone surrogate IS one code point, but never a member).
//! * `RENDERABLE_NAME_HAS_ALNUM_RE` is the unanchored `[<ALNUM>]` search
//!   with the `u` flag: true iff any code point is in the alnum set above
//!   (minus the four punctuation singles). Surrogate pairs are one code
//!   point above U+FFFF and never match.
//! * `GAME_ID_REGEX` has no `u` flag, so `isValidGameID` tests **UTF-16 code
//!   units**: length 8..=10 with every unit in `[0-9A-Za-z]` (a surrogate
//!   pair counts as two units and fails the class check).
//! * `LobbyInfoEvent` / `GroupTokenEvent` are pure field-storage classes with
//!   no methods and are not ported; `TEAM_COUNT_PRESETS` is module-private
//!   and only feeds the unported zod `select`.

/// `PublicGameTypeSchema.options` — the four lobby types.
pub const PUBLIC_GAME_TYPES: [&str; 4] = ["ffa", "team", "special", "hosted"];

/// `SCHEDULED_PUBLIC_GAME_TYPES` — `PublicGameTypeSchema.exclude(["hosted"])
/// .options`: the lobby types the master schedules from the map playlist.
pub const SCHEDULED_PUBLIC_GAME_TYPES: [&str; 3] = ["ffa", "team", "special"];

/// `LobbyAccentSchema.options` — the four featured-lobby accent colors.
pub const LOBBY_ACCENTS: [&str; 4] = ["gold", "blue", "green", "red"];

/// `ClientPlatformSchema.options` — the distribution platforms (zbin
/// ordinals, append-only).
pub const CLIENT_PLATFORMS: [&str; 3] = ["web", "steam", "crazygames"];

/// `ReportReasonSchema.options` — the closed report-reason set.
pub const REPORT_REASONS: [&str; 4] = [
    "botting",
    "teaming",
    "inappropriate_username",
    "griefing",
];

/// `LogSeverity` — the string-enum `(name, value)` table in declaration
/// order.
pub const LOG_SEVERITIES: [(&str, &str); 5] = [
    ("Debug", "DEBUG"),
    ("Info", "INFO"),
    ("Warn", "WARN"),
    ("Error", "ERROR"),
    ("Fatal", "FATAL"),
];

/// `QuickChatKeySchema.options` — every `"<category>.<key>"` derived from
/// `resources/QuickChat.json` in JSON insertion order (58 keys).
pub const QUICK_CHAT_KEYS: &[&str] = &[
    "help.troops",
    "help.troops_frontlines",
    "help.gold",
    "help.no_attack",
    "help.sorry_attack",
    "help.alliance",
    "help.help_defend",
    "help.trade_partners",
    "attack.attack",
    "attack.mirv",
    "attack.focus",
    "attack.finish",
    "attack.build_warships",
    "defend.defend",
    "defend.defend_from",
    "defend.dont_attack",
    "defend.ally",
    "defend.build_posts",
    "greet.hello",
    "greet.good_job",
    "greet.good_luck",
    "greet.have_fun",
    "greet.gg",
    "greet.nice_to_meet",
    "greet.well_played",
    "greet.hi_again",
    "greet.bye",
    "greet.thanks",
    "greet.oops",
    "greet.trust_me",
    "greet.trust_broken",
    "greet.ruining_games",
    "greet.dont_do_that",
    "greet.same_team",
    "misc.go",
    "misc.strategy",
    "misc.fun",
    "misc.team_up",
    "misc.pr",
    "misc.build_closer",
    "misc.coastline",
    "warnings.strong",
    "warnings.weak",
    "warnings.mirv_soon",
    "warnings.number1_warning",
    "warnings.stalemate",
    "warnings.has_allies",
    "warnings.no_allies",
    "warnings.betrayed",
    "warnings.betrayed_me",
    "warnings.getting_big",
    "warnings.danger_base",
    "warnings.saving_for_mirv",
    "warnings.mirv_ready",
    "warnings.snowballing",
    "warnings.cheating",
    "warnings.stop_trading",
    "warnings.stop_trading_all",
];

/// `MAX_HOSTED_LOBBIES` — cluster-wide cap on subscriber-listed lobbies.
pub const MAX_HOSTED_LOBBIES: usize = 10;
/// `HOSTED_LOBBY_AUTO_START_MS` — 5 minutes in ms.
pub const HOSTED_LOBBY_AUTO_START_MS: u64 = 5 * 60 * 1000;
/// `FEATURED_LOBBY_AUTO_START_MS` — 10 minutes in ms.
pub const FEATURED_LOBBY_AUTO_START_MS: u64 = 10 * 60 * 1000;

/// `CLIENT_ID_MAPPING` — the zbin dictionary name for player clientIDs.
pub const CLIENT_ID_MAPPING: &str = "clientId";
/// `ADMIN_BOT_CLIENT_ID` — the placeholder clientID stamped onto admin-bot
/// intents.
pub const ADMIN_BOT_CLIENT_ID: &str = "ADMINBOT";
/// `GAME_ID_REGEX.source`.
pub const GAME_ID_REGEX_SOURCE: &str = r"^[A-Za-z0-9]{8,10}$";
/// `RENDERABLE_NAME_ALNUM` — regex source text (literal backslash-u escapes).
pub const RENDERABLE_NAME_ALNUM: &str =
    r"a-zA-Z0-9\u00C0-\u00D6\u00D8-\u00F6\u00F8-\u00FF";
/// `RENDERABLE_NAME_CHARS` — `" _.\-"` prefix + the ALNUM source.
pub const RENDERABLE_NAME_CHARS: &str =
    concat!(" _.\\-", r"a-zA-Z0-9\u00C0-\u00D6\u00D8-\u00F6\u00F8-\u00FF");

// ---------------------------------------------------------------- predicates

/// `isValidGameID(value)` — `/^[A-Za-z0-9]{8,10}$/` with **no `u` flag**, so
/// JS tests UTF-16 code units: 8..=10 units, each an ASCII letter/digit.
pub fn is_valid_game_id(units: &[u16]) -> bool {
    (8..=10).contains(&units.len())
        && units
            .iter()
            .all(|&u| matches!(u, 0x30..=0x39 | 0x41..=0x5A | 0x61..=0x7A))
}

/// The `RENDERABLE_NAME_CHAR_RE` / `RENDERABLE_NAME_HAS_ALNUM_RE` character
/// class as a code-point set (see the module doc-comment for the probed
/// `\\-` resolution): ASCII alnum ∪ [C0,D6] ∪ [D8,F6] ∪ [F8,FF].
fn in_alnum_set(cp: u32) -> bool {
    matches!(
        cp,
        0x30..=0x39
            | 0x41..=0x5A
            | 0x61..=0x7A
            | 0xC0..=0xD6
            | 0xD8..=0xF6
            | 0xF8..=0xFF
    )
}

/// One code point of `RENDERABLE_NAME_CHAR_RE`'s class: the alnum set plus
/// the four singles space / `_` / `.` / `-`.
pub fn is_renderable_name_char(cp: u32) -> bool {
    // 0x20 space, 0x5F `_`, 0x2E `.`, 0x2D `-`.
    in_alnum_set(cp) || matches!(cp, 0x20 | 0x5F | 0x2E | 0x2D)
}

/// `RENDERABLE_NAME_CHAR_RE.test(s)` — the anchored `u`-flag pattern matches
/// iff `s` is exactly one code point and that code point is in the class.
pub fn renderable_name_char_test(units: &[u16]) -> bool {
    if units.is_empty() {
        return false;
    }
    let (cp, w) = next_code_point(units, 0);
    w == units.len() && is_renderable_name_char(cp)
}

/// `RENDERABLE_NAME_HAS_ALNUM_RE.test(s)` — the unanchored `u`-flag search:
/// true iff any code point of `s` is in the alnum set.
pub fn has_renderable_alnum(units: &[u16]) -> bool {
    let mut i = 0;
    while i < units.len() {
        let (cp, w) = next_code_point(units, i);
        if in_alnum_set(cp) {
            return true;
        }
        i += w;
    }
    false
}

/// UTF-16 → code point step with JS `for...of` semantics: a valid surrogate
/// pair decodes to one code point; a lone high surrogate (or an unpaired
/// low surrogate) is its own code point. Mirrors the module-private helper
/// in `util.rs` (kept local to avoid widening that module's visibility).
fn next_code_point(units: &[u16], i: usize) -> (u32, usize) {
    let u = units[i];
    if (0xD800..=0xDBFF).contains(&u) && i + 1 < units.len() && (0xDC00..=0xDFFF).contains(&units[i + 1]) {
        let cp = 0x1_0000 + ((u as u32 - 0xD800) << 10) + (units[i + 1] as u32 - 0xDC00);
        (cp, 2)
    } else {
        (u as u32, 1)
    }
}

// ---------------------------------------------------------------- vectors op
//
// A flat `f64` token runner shared by the golden replay and the wasm probe.
// Strings cross as `[len, u0, .. u(len-1)]` (UTF-16 code units).
//
// kind table (mirrors the capture):
//   0 [0] -> [4, (str)*4]                PUBLIC_GAME_TYPES dump
//   1 [0] -> [3, (str)*3]                SCHEDULED_PUBLIC_GAME_TYPES dump
//   2 [0] -> [4, (str)*4]                LOBBY_ACCENTS dump
//   3 [0] -> [3, (str)*3]                CLIENT_PLATFORMS dump
//   4 [0] -> [4, (str)*4]                REPORT_REASONS dump
//   5 [0] -> [3, (f64)*3]                numeric lobby constants
//   6 [0] -> [5, (str)*5]                string constants
//   7 [0] -> [5, (name,val)*5]           LOG_SEVERITIES dump
//   8 [0] -> [n, (str)*n]                QUICK_CHAT_KEYS dump
//   9 [n,(str)*n] -> [n,(0/1)*n]         isValidGameID batch
//  10 [n,(str)*n] -> [n,(0/1)*n]         RENDERABLE_NAME_CHAR_RE.test batch
//  11 [n,(str)*n] -> [n,(0/1)*n]         RENDERABLE_NAME_HAS_ALNUM_RE.test batch

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
    fn units(&mut self) -> Vec<u16> {
        let len = self.u();
        (0..len).map(|_| self.f() as u16).collect()
    }
}

fn push_string(out: &mut Vec<f64>, s: &str) {
    let units: Vec<u16> = s.encode_utf16().collect();
    out.push(units.len() as f64);
    out.extend(units.iter().map(|&u| u as f64));
}

fn push_batch(
    out: &mut Vec<f64>,
    c: &mut Cur,
    f: fn(&[u16]) -> bool,
) {
    let n = c.u();
    out.push(n as f64);
    for _ in 0..n {
        let s = c.units();
        out.push(if f(&s) { 1.0 } else { 0.0 });
    }
}

pub fn run_op(kind: u8, args: &[f64]) -> Vec<f64> {
    let mut out = Vec::new();
    let mut c = Cur(args, 0);
    match kind {
        0 => {
            out.push(PUBLIC_GAME_TYPES.len() as f64);
            for e in PUBLIC_GAME_TYPES {
                push_string(&mut out, e);
            }
        }
        1 => {
            out.push(SCHEDULED_PUBLIC_GAME_TYPES.len() as f64);
            for e in SCHEDULED_PUBLIC_GAME_TYPES {
                push_string(&mut out, e);
            }
        }
        2 => {
            out.push(LOBBY_ACCENTS.len() as f64);
            for e in LOBBY_ACCENTS {
                push_string(&mut out, e);
            }
        }
        3 => {
            out.push(CLIENT_PLATFORMS.len() as f64);
            for e in CLIENT_PLATFORMS {
                push_string(&mut out, e);
            }
        }
        4 => {
            out.push(REPORT_REASONS.len() as f64);
            for e in REPORT_REASONS {
                push_string(&mut out, e);
            }
        }
        5 => {
            out.push(3.0);
            out.push(MAX_HOSTED_LOBBIES as f64);
            out.push(HOSTED_LOBBY_AUTO_START_MS as f64);
            out.push(FEATURED_LOBBY_AUTO_START_MS as f64);
        }
        6 => {
            out.push(5.0);
            push_string(&mut out, CLIENT_ID_MAPPING);
            push_string(&mut out, ADMIN_BOT_CLIENT_ID);
            push_string(&mut out, GAME_ID_REGEX_SOURCE);
            push_string(&mut out, RENDERABLE_NAME_ALNUM);
            push_string(&mut out, RENDERABLE_NAME_CHARS);
        }
        7 => {
            out.push(LOG_SEVERITIES.len() as f64);
            for (n, v) in LOG_SEVERITIES {
                push_string(&mut out, n);
                push_string(&mut out, v);
            }
        }
        8 => {
            out.push(QUICK_CHAT_KEYS.len() as f64);
            for e in QUICK_CHAT_KEYS {
                push_string(&mut out, e);
            }
        }
        9 => push_batch(&mut out, &mut c, is_valid_game_id),
        10 => push_batch(&mut out, &mut c, renderable_name_char_test),
        11 => push_batch(&mut out, &mut c, has_renderable_alnum),
        _ => {}
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn units(s: &str) -> Vec<u16> {
        s.encode_utf16().collect()
    }

    fn enc_str(s: &str) -> Vec<f64> {
        let u: Vec<u16> = s.encode_utf16().collect();
        let mut v = vec![u.len() as f64];
        v.extend(u.iter().map(|&x| x as f64));
        v
    }

    #[test]
    fn const_shapes() {
        assert_eq!(PUBLIC_GAME_TYPES, ["ffa", "team", "special", "hosted"]);
        assert_eq!(SCHEDULED_PUBLIC_GAME_TYPES, ["ffa", "team", "special"]);
        assert_eq!(LOBBY_ACCENTS, ["gold", "blue", "green", "red"]);
        assert_eq!(CLIENT_PLATFORMS, ["web", "steam", "crazygames"]);
        assert_eq!(REPORT_REASONS.len(), 4);
        assert_eq!(LOG_SEVERITIES.len(), 5);
        assert_eq!(LOG_SEVERITIES[0], ("Debug", "DEBUG"));
        assert_eq!(LOG_SEVERITIES[4], ("Fatal", "FATAL"));
        assert_eq!(QUICK_CHAT_KEYS.len(), 58);
        assert_eq!(QUICK_CHAT_KEYS[0], "help.troops");
        assert_eq!(QUICK_CHAT_KEYS[57], "warnings.stop_trading_all");
        assert_eq!(HOSTED_LOBBY_AUTO_START_MS, 300_000);
        assert_eq!(FEATURED_LOBBY_AUTO_START_MS, 600_000);
        // The regex sources carry literal backslash-u escape text.
        assert_eq!(
            RENDERABLE_NAME_ALNUM,
            "a-zA-Z0-9\\u00C0-\\u00D6\\u00D8-\\u00F6\\u00F8-\\u00FF"
        );
        assert_eq!(
            RENDERABLE_NAME_CHARS,
            format!(" _.\\-{}", RENDERABLE_NAME_ALNUM)
        );
        assert_eq!(GAME_ID_REGEX_SOURCE, "^[A-Za-z0-9]{8,10}$");
    }

    #[test]
    fn game_id_boundaries() {
        assert!(!is_valid_game_id(&units("")));
        assert!(!is_valid_game_id(&units("abcdefg"))); // 7
        assert!(is_valid_game_id(&units("abcdefgh"))); // 8
        assert!(is_valid_game_id(&units("abcdefghi"))); // 9
        assert!(is_valid_game_id(&units("AbCdEfGhIj"))); // 10
        assert!(!is_valid_game_id(&units("abcdefghijk"))); // 11
        assert!(!is_valid_game_id(&units("abcdefg_")));
        assert!(!is_valid_game_id(&units("abcd 123")));
        assert!(is_valid_game_id(&units("ADMINBOT"))); // 8 letters, class-legal
        // No u flag: code-unit iteration.
        assert!(!is_valid_game_id(&units("\u{1F600}abcdefg"))); // 10 units, surrogate pair
        assert!(!is_valid_game_id(&units("Àbcdefgh")));
    }

    #[test]
    fn char_class_membership() {
        // Probed truth table: `\\-` is literal `-`, NOT the U+005C-U+0061 range.
        for ch in [' ', '_', '.', '-'] {
            assert!(is_renderable_name_char(ch as u32), "{ch}");
        }
        for ch in ['[', ']', '^', '`', '\\', '{', '|', '~', '/', ':', ';', '!', '+'] {
            assert!(!is_renderable_name_char(ch as u32), "{ch}");
        }
        assert!(is_renderable_name_char('a' as u32));
        assert!(is_renderable_name_char('Z' as u32));
        assert!(is_renderable_name_char('5' as u32));
        // Latin-1 boundaries.
        assert!(!is_renderable_name_char(0xBF));
        assert!(is_renderable_name_char(0xC0));
        assert!(is_renderable_name_char(0xD6));
        assert!(!is_renderable_name_char(0xD7));
        assert!(is_renderable_name_char(0xD8));
        assert!(is_renderable_name_char(0xF6));
        assert!(!is_renderable_name_char(0xF7));
        assert!(is_renderable_name_char(0xF8));
        assert!(is_renderable_name_char(0xFF));
        assert!(!is_renderable_name_char(0x100));
    }

    #[test]
    fn char_test_anchoring() {
        assert!(!renderable_name_char_test(&units("")));
        assert!(renderable_name_char_test(&units("a")));
        assert!(renderable_name_char_test(&units(" ")));
        assert!(!renderable_name_char_test(&units("ab")));
        assert!(!renderable_name_char_test(&units("a ")));
        assert!(!renderable_name_char_test(&units("\u{1F600}"))); // 1 cp, not in set
        // Lone high surrogate: one code point, not in the set.
        assert!(!renderable_name_char_test(&[0xD83D]));
        assert!(!renderable_name_char_test(&[0xD83D, 0x41])); // 2 cps
    }

    #[test]
    fn has_alnum_search() {
        assert!(!has_renderable_alnum(&units("")));
        assert!(!has_renderable_alnum(&units("  ")));
        assert!(!has_renderable_alnum(&units("-_.")));
        assert!(has_renderable_alnum(&units("  a")));
        assert!(has_renderable_alnum(&units("\u{1F600}a")));
        assert!(!has_renderable_alnum(&units("\u{1F600}"))); // pair = cp 0x1F600
        assert!(!has_renderable_alnum(&[0xD83D])); // lone surrogate
        assert!(has_renderable_alnum(&[0xD83D, 0x41]));
    }

    #[test]
    fn run_op_dump_shapes() {
        assert_eq!(run_op(0, &[0.0])[0], 4.0);
        assert_eq!(run_op(1, &[0.0])[0], 3.0);
        assert_eq!(run_op(2, &[0.0])[0], 4.0);
        assert_eq!(run_op(3, &[0.0])[0], 3.0);
        assert_eq!(run_op(4, &[0.0])[0], 4.0);
        assert_eq!(run_op(5, &[0.0]), vec![3.0, 10.0, 300000.0, 600000.0]);
        let s = run_op(6, &[0.0]);
        assert_eq!(s[0], 5.0);
        assert_eq!(s[1], 8.0); // "clientId" length
        let l = run_op(7, &[0.0]);
        assert_eq!(l[0], 5.0);
        let q = run_op(8, &[0.0]);
        assert_eq!(q[0], 58.0);
    }

    #[test]
    fn run_op_batches() {
        let mut args = vec![2.0];
        args.extend(enc_str("abcdefgh"));
        args.extend(enc_str("abcdefg"));
        let res = run_op(9, &args);
        assert_eq!(res, vec![2.0, 1.0, 0.0]);

        let mut args = vec![3.0];
        args.extend(enc_str("a"));
        args.extend(enc_str("ab"));
        args.extend(enc_str(""));
        let res = run_op(10, &args);
        assert_eq!(res, vec![3.0, 1.0, 0.0, 0.0]);

        let mut args = vec![2.0];
        args.extend(enc_str("  "));
        args.extend(enc_str("  a"));
        let res = run_op(11, &args);
        assert_eq!(res, vec![2.0, 0.0, 1.0]);
    }
}
