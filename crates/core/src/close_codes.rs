//! Port of `src/core/CloseCodes.ts`.
//!
//! The WebSocket close-code / close-reason vocabulary plus the two predicates
//! that classify a code or reason. Both tables are part of the wire contract,
//! so the **entry order** and the exact numeric / string values are pinned.
//!
//! Faithfulness notes:
//!
//! * [`is_terminal_close`] mirrors the JS expression verbatim: `code === 1000
//!   || code === 1002 || (code >= 4000 && code <= 4999)`. Every comparison is
//!   an IEEE `f64` comparison, so `NaN` (never `==`-equal, never ordered) and
//!   `±Infinity` (fails the `<= 4999` / `>= 4000` half of the range test) all
//!   return `false`, and `-0` is `==` to `0` but not to any listed code. A
//!   fractional code inside `[4000, 4999]` (e.g. `4000.5`) is `true`, exactly
//!   as in JS — the range test is on the raw number, not an integer cast.
//! * [`is_close_reason`] is a JS `Set#has` over the reason strings; the keys
//!   are all distinct plain strings, so SameValueZero reduces to string
//!   equality. The empty string and any near-miss (wrong case, prefix) are
//!   `false`.

/// One `CloseCode` entry: `(name, value)` in TS declaration order.
pub const CLOSE_CODE_VALUES: [(&str, f64); 16] = [
    ("Normal", 1000.0),
    ("ProtocolError", 1002.0),
    ("InternalError", 1011.0),
    ("TryAgainLater", 1013.0),
    ("BadRequest", 4000.0),
    ("Unauthorized", 4001.0),
    ("Forbidden", 4002.0),
    ("Banned", 4003.0),
    ("GameNotFound", 4004.0),
    ("GameClosed", 4005.0),
    ("LobbyFull", 4006.0),
    ("WrongWorker", 4007.0),
    ("GameStarted", 4008.0),
    ("RankedLimitReached", 4100.0),
    ("InvalidClan", 4101.0),
    ("ClanVerificationFailed", 4102.0),
];

/// `APP_REJECTION_MIN`.
pub const APP_REJECTION_MIN: f64 = 4000.0;
/// `APP_REJECTION_MAX`.
pub const APP_REJECTION_MAX: f64 = 4999.0;

/// One `CloseReason` value, in TS declaration order (the order `Object.values`
/// yields, which seeds the membership `Set`).
pub const CLOSE_REASON_VALUES: [&str; 23] = [
    "close_reason.invalid_message",
    "close_reason.invalid_token",
    "close_reason.banned",
    "close_reason.turnstile_failed",
    "close_reason.login_required",
    "close_reason.account_lookup_failed",
    "close_reason.forbidden",
    "close_reason.cosmetics_forbidden",
    "close_reason.game_not_found",
    "close_reason.cannot_join",
    "close_reason.not_allowlisted",
    "close_reason.not_trusted",
    "close_reason.lobby_full",
    "close_reason.wrong_worker",
    "close_reason.internal_error",
    "close_reason.protocol_error",
    "close_reason.no_heartbeat",
    "close_reason.game_ended",
    "close_reason.game_started",
    "close_reason.ranked_limit_reached",
    "close_reason.invalid_clan",
    "close_reason.clan_verification_failed",
    "close_reason.unknown",
];

/// `isTerminalClose(code)`.
pub fn is_terminal_close(code: f64) -> bool {
    code == 1000.0 || code == 1002.0 || (APP_REJECTION_MIN..=APP_REJECTION_MAX).contains(&code)
}

/// `isCloseReason(value)`.
pub fn is_close_reason(value: &str) -> bool {
    CLOSE_REASON_VALUES.contains(&value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_codes() {
        assert!(is_terminal_close(1000.0));
        assert!(is_terminal_close(1002.0));
        assert!(is_terminal_close(4000.0));
        assert!(is_terminal_close(4999.0));
        assert!(is_terminal_close(4102.0));
        // Inside the app range but fractional — still terminal in JS.
        assert!(is_terminal_close(4000.5));
        assert!(is_terminal_close(4998.75));
    }

    #[test]
    fn non_terminal_codes() {
        assert!(!is_terminal_close(1011.0));
        assert!(!is_terminal_close(1013.0));
        assert!(!is_terminal_close(3999.0));
        assert!(!is_terminal_close(5000.0));
        assert!(!is_terminal_close(0.0));
        assert!(!is_terminal_close(-0.0));
        assert!(!is_terminal_close(1000.5));
    }

    #[test]
    fn non_finite_codes_are_not_terminal() {
        assert!(!is_terminal_close(f64::NAN));
        assert!(!is_terminal_close(f64::INFINITY));
        assert!(!is_terminal_close(f64::NEG_INFINITY));
    }

    #[test]
    fn reason_membership() {
        assert!(is_close_reason("close_reason.unknown"));
        assert!(is_close_reason("close_reason.invalid_message"));
        assert!(is_close_reason("close_reason.clan_verification_failed"));
        assert!(!is_close_reason(""));
        assert!(!is_close_reason("close_reason.does_not_exist"));
        // Case-sensitive and exact-match only.
        assert!(!is_close_reason("Close_Reason.Unknown"));
        assert!(!is_close_reason("close_reason"));
        assert!(!is_close_reason("close_reason.unknown "));
    }

    #[test]
    fn tables_have_expected_shape() {
        assert_eq!(CLOSE_CODE_VALUES.len(), 16);
        assert_eq!(CLOSE_REASON_VALUES.len(), 23);
        // The app-range codes are the tail of the code table.
        assert_eq!(CLOSE_CODE_VALUES[4].1, APP_REJECTION_MIN);
        for &(_, v) in CLOSE_CODE_VALUES.iter().filter(|(_, v)| *v >= 4000.0) {
            assert!(is_terminal_close(v), "{v} should be terminal");
        }
    }
}
