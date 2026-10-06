//! Port of `src/client/SubscriptionPolicy.ts` — the Steam subscription-rail
//! launch policy switch. One pure `const` (false at launch); the panel and
//! the store read the SAME switch. `STEAM_TIER_CHANGE_IN_APP = false`.
//!
//! The capture dumps the single boolean.

/// `STEAM_TIER_CHANGE_IN_APP` — BLOCKED at launch (S1: no in-app tier change
/// on the Steam rail).
pub const STEAM_TIER_CHANGE_IN_APP: bool = false;

/// `run_op(kind, args)` — capture harness entry (stateless). Kind table:
/// 0 -> `[0]` (STEAM_TIER_CHANGE_IN_APP === false → 0; true would be 1).
pub fn run_op(kind: u8, _args: &[f64]) -> Vec<f64> {
    match kind {
        0 => vec![if STEAM_TIER_CHANGE_IN_APP { 1.0 } else { 0.0 }],
        k => unreachable!("subscription_policy: unknown op kind {k}"),
    }
}
