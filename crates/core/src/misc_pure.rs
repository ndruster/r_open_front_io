//! Port of two tiny pure helpers the client shares:
//!
//! * `isFfa` from `src/client/components/baseComponents/stats/GameTypeLabels.ts`
//!   (the `formatGameType` half is `translateText` / intl-bound and stays out
//!   of scope), and
//! * `cardClass` from `src/client/components/InputCardStyles.ts`.
//!
//! Faithfulness notes (quirk list):
//!
//! * `isFfa(game)`: `game.mode === GameMode.FFA` (the string `"Free For
//!   All"`, a strict `===`) short-circuits to true. Otherwise true only when
//!   `game.mode === undefined` AND `playerTeams` is nullish — a null WITH a
//!   mode is still a Team game (the legacy `player_teams = NULL` rows).
//!   `mode` present-but-other -> false regardless of `playerTeams`.
//! * `cardClass(active, extra = "")`: the default parameter fires ONLY on
//!   `undefined` — an explicit `""` produces the same string but an explicit
//!   other value survives. The template embeds `${extra}` and the active /
//!   inactive class with single spaces, so `extra` omitted and `extra === ""`
//!   both leave a double space between the fixed prefix and the card class.

use crate::js_json::{push_str, read_str, read_val, JsVal};

/// `GameMode.FFA` — the string-enum value.
pub const GAME_MODE_FFA: &str = "Free For All";

/// `ACTIVE_CARD` from InputCardStyles.ts (verbatim, spaces included).
pub const ACTIVE_CARD: &str =
    "bg-malibu-blue/20 border-malibu-blue/50 shadow-[var(--shadow-malibu-blue)]";

/// `INACTIVE_CARD` from InputCardStyles.ts (verbatim, spaces included).
pub const INACTIVE_CARD: &str =
    "bg-white/5 border-white/10 hover:bg-white/10 hover:border-white/20";

/// The fixed template prefix (everything before the first `${extra}`).
pub const CARD_PREFIX: &str = "w-full h-full rounded-xl border cursor-pointer transition-all duration-200 active:scale-95 relative overflow-hidden ";

/// `isFfa(game)` over the `mode` / `playerTeams` JsVal domain.
pub fn is_ffa(mode: &JsVal, player_teams: &JsVal) -> bool {
    if matches!(mode, JsVal::Str(s) if s == GAME_MODE_FFA) {
        return true;
    }
    if matches!(mode, JsVal::Absent | JsVal::Undef)
        && matches!(player_teams, JsVal::Absent | JsVal::Undef | JsVal::Null)
    {
        return true;
    }
    false
}

/// `cardClass(active, extra)` — `extra` is `None` only for the `undefined`
/// default-parameter case; the TS result is identical for `None` and
/// `Some("")` (the template just interpolates `""`).
pub fn card_class(active: bool, extra: Option<&str>) -> String {
    let e = extra.unwrap_or("");
    let tail = if active { ACTIVE_CARD } else { INACTIVE_CARD };
    format!("{CARD_PREFIX}{e} {tail}")
}

// ---------------------------------------------------------------- vectors op
//
// kind 0: [n, (encVal mode, encVal playerTeams)*n] -> (0|1)*n
//         isFfa batch (Absent models a missing key, Undef a present
//         undefined).
// kind 1: [n, (bool active, extraTag 0|1, extra?)*n] -> (encS class)*n
//         cardClass batch; extraTag 0 = the default parameter (undefined),
//         1 = an explicit string.
// kind 2: [] -> [encS(ACTIVE_CARD), encS(INACTIVE_CARD), encS(CARD_PREFIX),
//         encS(GAME_MODE_FFA)]  the constant dump.

pub fn run_op(kind: u8, args: &[f64]) -> Vec<f64> {
    let mut out = Vec::new();
    let mut i = 0usize;
    match kind {
        0 => {
            let n = args[i] as usize;
            i += 1;
            for _ in 0..n {
                let mode = read_val(args, &mut i);
                let pt = read_val(args, &mut i);
                out.push(if is_ffa(&mode, &pt) { 1.0 } else { 0.0 });
            }
        }
        1 => {
            let n = args[i] as usize;
            i += 1;
            for _ in 0..n {
                let active = args[i] != 0.0;
                i += 1;
                let extra = if args[i] == 0.0 {
                    i += 1;
                    None
                } else {
                    i += 1;
                    Some(read_str(args, &mut i))
                };
                push_str(&mut out, &card_class(active, extra.as_deref()));
            }
        }
        2 => {
            push_str(&mut out, ACTIVE_CARD);
            push_str(&mut out, INACTIVE_CARD);
            push_str(&mut out, CARD_PREFIX);
            push_str(&mut out, GAME_MODE_FFA);
        }
        k => unreachable!("misc_pure: unknown op kind {k}"),
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &str) -> JsVal {
        JsVal::Str(v.into())
    }

    #[test]
    fn is_ffa_mode_short_circuit() {
        assert!(is_ffa(&s(GAME_MODE_FFA), &JsVal::Null));
        assert!(is_ffa(&s(GAME_MODE_FFA), &JsVal::Str("2".into())));
        // mode present but other -> false even with a nullish playerTeams.
        assert!(!is_ffa(&s("Team"), &JsVal::Null));
        assert!(!is_ffa(&s("Team"), &JsVal::Absent));
    }

    #[test]
    fn is_ffa_absent_mode_nullish_teams() {
        assert!(is_ffa(&JsVal::Absent, &JsVal::Null));
        assert!(is_ffa(&JsVal::Absent, &JsVal::Absent));
        assert!(is_ffa(&JsVal::Undef, &JsVal::Undef));
        // A string playerTeams blocks the fallback.
        assert!(!is_ffa(&JsVal::Absent, &JsVal::Str("Duos".into())));
        // The nullish gate is strict: "" / 0 are NOT nullish.
        assert!(!is_ffa(&JsVal::Absent, &JsVal::Str("".into())));
        assert!(!is_ffa(&JsVal::Absent, &JsVal::Num(0.0)));
    }

    #[test]
    fn card_class_template_spaces() {
        let on = card_class(true, None);
        assert!(on.starts_with(CARD_PREFIX));
        assert!(on.ends_with(ACTIVE_CARD));
        assert!(on.contains(&format!("{CARD_PREFIX} {ACTIVE_CARD}")));
        // The default parameter and an explicit "" agree...
        assert_eq!(on, card_class(true, Some("")));
        // ...but an explicit value survives.
        let x = card_class(false, Some("px-2"));
        assert!(x.contains(" overflow-hidden px-2 bg-white/5"));
        assert_ne!(card_class(false, None), x);
    }

    #[test]
    fn run_op_kind1_roundtrip() {
        // [active=true, default extra] -> one encS string.
        let r = run_op(1, &[1.0, 1.0, 0.0]);
        let mut want = Vec::new();
        push_str(&mut want, &card_class(true, None));
        assert_eq!(r, want);
    }
}
