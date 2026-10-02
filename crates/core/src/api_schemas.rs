//! Port of the runtime-value subset of `src/core/ApiSchemas.ts`: the data
//! constants (`ADMIN_ROLES`, `PlayerStatsGameModes`, the two player game
//! filter arrays), the `z.enum` option arrays, and the four pure predicates
//! (`isAdminRole`, `isTemporaryUsername`, `isVerifiedUsername`,
//! `isGrantedSubscription`).
//!
//! Scope: the plain data and pure predicates the API wire layer is built
//! from. Faithfulness notes:
//!
//! * The `z.object(...)` schema declarations are wire-validation and are not
//!   ported — only their closed-set option arrays are (the capture reads
//!   `.options` off a functional zod shim; everything else rides an inert
//!   Proxy, including `RequiredClanTagSchema = ClanTagSchema.unwrap()`).
//! * `TokenPayloadSchema`'s refine / transform callbacks reference
//!   `base64urlToUuid` from `Base64.ts` (a `jose` dependency) and are never
//!   invoked at module load; the capture stubs the import. Not ported.
//! * `PlayerStatsGameModes` is captured as the *string values* the TS array
//!   holds at runtime (`GameMode.FFA`, `GameMode.Team`, and the bare
//!   `HumansVsNations` constant), hard-coded below and pinned by the parity
//!   dump.
//! * `isAdminRole` compares with JS `===` against the two literals: any
//!   non-`admin`/`root` string (and case variants) is false.
//! * `isTemporaryUsername` is `/^TEMPORARY\d{4}$/.test(base)` with **no `u`
//!   flag**: the test runs over UTF-16 code units and `\d` is ASCII `0-9`
//!   only (Arabic-Indic digits do not match). Anchored, so exactly ten
//!   `TEMPORARY` units followed by exactly four ASCII digits.
//! * `isVerifiedUsername` is `typeof === "string" && !includes(".") &&
//!   !isTemporaryUsername(...)`; `includes` scans code units and the dot is
//!   only ever U+002E.
//! * `isGrantedSubscription` applies the three-state `provider` rule: `null`
//!   means granted (true), any string means paid (false), a missing field
//!   means the server predates the feature (false), and no subscription at
//!   all is false. The runner models the three states through the capture's
//!   sub encoding.

/// `ADMIN_ROLES` — the roles the API treats as administrative.
pub const ADMIN_ROLES: [&str; 2] = ["admin", "root"];

/// `PlayerStatsGameModes` — the modes the player-stats tree is bucketed by,
/// as the runtime string values (`GameMode.FFA`, `GameMode.Team`,
/// `HumansVsNations`).
pub const PLAYER_STATS_GAME_MODES: [&str; 3] = ["Free For All", "Team", "Humans Vs Nations"];

/// `PlayerGameModeFilters` — the mode buckets for the public games history.
pub const PLAYER_GAME_MODE_FILTERS: [&str; 4] = ["ffa", "team", "hvn", "ranked"];

/// `PlayerGameTypeFilters` — the game-type split (matches `games.type`).
pub const PLAYER_GAME_TYPE_FILTERS: [&str; 3] = ["public", "private", "singleplayer"];

/// `UsernameStatusSchema.options` — the account-username lifecycle states.
pub const USERNAME_STATUS_OPTIONS: [&str; 4] =
    ["unclaimed", "claimed", "premium", "indefinite"];

/// `BareClaimSchema.options` — what happened to the bare-name claim.
pub const BARE_CLAIM_OPTIONS: [&str; 3] = ["claimed", "unavailable", "not_eligible"];

/// `TribeNameStatusSchema.options` — the backend moderation states.
pub const TRIBE_NAME_STATUS_OPTIONS: [&str; 4] = ["pending", "live", "rejected", "revoked"];

/// `PlayerGameModeFilterSchema.options` — same array as the data constant.
pub const PLAYER_GAME_MODE_FILTER_OPTIONS: [&str; 4] = PLAYER_GAME_MODE_FILTERS;

/// `PlayerGameTypeFilterSchema.options` — same array as the data constant.
pub const PLAYER_GAME_TYPE_FILTER_OPTIONS: [&str; 3] = PLAYER_GAME_TYPE_FILTERS;

/// `PlayerGameResultSchema.options` — "incomplete" covers no-recorded-winner.
pub const PLAYER_GAME_RESULT_OPTIONS: [&str; 3] = ["victory", "defeat", "incomplete"];

/// `PaymentsProviderSchema.options` — the two billing rails.
pub const PAYMENTS_PROVIDER_OPTIONS: [&str; 2] = ["steam", "stripe"];

/// `PaymentsKindSchema.options` — the checkout listing kinds.
pub const PAYMENTS_KIND_OPTIONS: [&str; 3] =
    ["currency_pack", "custom_currency", "subscription_tier"];

/// `PaymentsHandoffSchema.options` — how the player is handed to the rail.
pub const PAYMENTS_HANDOFF_OPTIONS: [&str; 3] = ["redirect", "client_overlay", "client_secret"];

/// `SteamOrderResolutionSchema.options` — the finalize resolutions.
pub const STEAM_ORDER_RESOLUTION_OPTIONS: [&str; 4] =
    ["settled", "expired", "open", "unresolved"];

// ---------------------------------------------------------------- predicates

fn eq_utf16(units: &[u16], s: &str) -> bool {
    units.len() == s.len() && units.iter().copied().eq(s.encode_utf16())
}

/// `isAdminRole(role)` — JS `role === "admin" || role === "root"` over UTF-16
/// code units (a non-string never reaches the comparison; the runner's
/// string batches are always strings).
pub fn is_admin_role(units: &[u16]) -> bool {
    eq_utf16(units, "admin") || eq_utf16(units, "root")
}

/// `isTemporaryUsername(base)` — `/^TEMPORARY\d{4}$/` without the `u` flag:
/// exactly nine `TEMPORARY` code units followed by exactly four ASCII digits
/// (length 13; Arabic-Indic digits do not match `\d`).
pub fn is_temporary_username(units: &[u16]) -> bool {
    units.len() == 13
        && eq_utf16(&units[..9], "TEMPORARY")
        && units[9..].iter().all(|&u| (0x30..=0x39).contains(&u))
}

/// `isVerifiedUsername(username)` — a string with no `.` code unit and not a
/// `TEMPORARY####` server rename.
pub fn is_verified_username(units: &[u16]) -> bool {
    !units.contains(&0x2E) && !is_temporary_username(units)
}

/// The three-state `provider` modelling of a `UserSubscription` for
/// `isGrantedSubscription`: absent field / a string rail / explicitly null.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SubscriptionProvider<'a> {
    /// No `provider` key — the server predates the field.
    Missing,
    /// `provider` is a string ("stripe", "steam", or any future rail).
    Rail(&'a [u16]),
    /// `provider` is exactly `null` — a grant.
    Null,
}

/// `isGrantedSubscription(sub)` — `sub == null ? false : sub.provider ===
/// null`. The runner decodes the capture's sub encoding into the option of
/// this enum; `None` models `null` / `undefined`.
pub fn is_granted_subscription(sub: Option<SubscriptionProvider<'_>>) -> bool {
    matches!(sub, Some(SubscriptionProvider::Null))
}

// ---------------------------------------------------------------- vectors op
//
// A flat `f64` token runner shared by the golden replay and the wasm probe.
// Strings cross as `[len, u0, .. u(len-1)]` (UTF-16 code units).
//
// kind table (mirrors the capture):
//   0 [0] -> [2, (str)*2]              ADMIN_ROLES dump
//   1 [0] -> [3, (str)*3]              PlayerStatsGameModes dump
//   2 [0] -> [4, (str)*4]              PlayerGameModeFilters dump
//   3 [0] -> [3, (str)*3]              PlayerGameTypeFilters dump
//   4 [0] -> [4, (str)*4]              UsernameStatusSchema.options dump
//   5 [0] -> [3, (str)*3]              BareClaimSchema.options dump
//   6 [0] -> [4, (str)*4]              TribeNameStatusSchema.options dump
//   7 [0] -> [3, (str)*3]              PlayerGameResultSchema.options dump
//   8 [0] -> [2, (str)*2]              PaymentsProviderSchema.options dump
//   9 [0] -> [3, (str)*3]              PaymentsKindSchema.options dump
//  10 [0] -> [3, (str)*3]              PaymentsHandoffSchema.options dump
//  11 [0] -> [4, (str)*4]              SteamOrderResolutionSchema.options dump
//  12 [n, (str)*n] -> [n, (0/1)*n]     isAdminRole batch
//  13 [n, (str)*n] -> [n, (0/1)*n]     isTemporaryUsername batch
//  14 [n, (str)*n] -> [n, (0/1)*n]     isVerifiedUsername batch
//  15 [n, (enc sub)*n] -> [n, (0/1)*n] isGrantedSubscription batch
//     enc sub: [0]=undefined, [1, (str)provider]=string, [2]=provider null

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

fn push_batch(out: &mut Vec<f64>, c: &mut Cur, f: fn(&[u16]) -> bool) {
    let n = c.u();
    out.push(n as f64);
    for _ in 0..n {
        let s = c.units();
        out.push(if f(&s) { 1.0 } else { 0.0 });
    }
}

fn push_dump(out: &mut Vec<f64>, arr: &[&str]) {
    out.push(arr.len() as f64);
    for e in arr {
        push_string(out, e);
    }
}

pub fn run_op(kind: u8, args: &[f64]) -> Vec<f64> {
    let mut out = Vec::new();
    let mut c = Cur(args, 0);
    match kind {
        0 => push_dump(&mut out, &ADMIN_ROLES),
        1 => push_dump(&mut out, &PLAYER_STATS_GAME_MODES),
        2 => push_dump(&mut out, &PLAYER_GAME_MODE_FILTERS),
        3 => push_dump(&mut out, &PLAYER_GAME_TYPE_FILTERS),
        4 => push_dump(&mut out, &USERNAME_STATUS_OPTIONS),
        5 => push_dump(&mut out, &BARE_CLAIM_OPTIONS),
        6 => push_dump(&mut out, &TRIBE_NAME_STATUS_OPTIONS),
        7 => push_dump(&mut out, &PLAYER_GAME_RESULT_OPTIONS),
        8 => push_dump(&mut out, &PAYMENTS_PROVIDER_OPTIONS),
        9 => push_dump(&mut out, &PAYMENTS_KIND_OPTIONS),
        10 => push_dump(&mut out, &PAYMENTS_HANDOFF_OPTIONS),
        11 => push_dump(&mut out, &STEAM_ORDER_RESOLUTION_OPTIONS),
        12 => push_batch(&mut out, &mut c, is_admin_role),
        13 => push_batch(&mut out, &mut c, is_temporary_username),
        14 => push_batch(&mut out, &mut c, is_verified_username),
        15 => {
            let n = c.u();
            out.push(n as f64);
            for _ in 0..n {
                let tag = c.u();
                let granted = match tag {
                    0 => is_granted_subscription(None),
                    2 => is_granted_subscription(Some(SubscriptionProvider::Null)),
                    // [1, len, u0, ..]: the provider string value.
                    _ => {
                        let len = c.u();
                        let rail: Vec<u16> = (0..len).map(|_| c.f() as u16).collect();
                        is_granted_subscription(Some(SubscriptionProvider::Rail(&rail)))
                    }
                };
                out.push(if granted { 1.0 } else { 0.0 });
            }
        }
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
        assert_eq!(ADMIN_ROLES, ["admin", "root"]);
        assert_eq!(
            PLAYER_STATS_GAME_MODES,
            ["Free For All", "Team", "Humans Vs Nations"]
        );
        assert_eq!(PLAYER_GAME_MODE_FILTERS, ["ffa", "team", "hvn", "ranked"]);
        assert_eq!(PLAYER_GAME_TYPE_FILTERS, ["public", "private", "singleplayer"]);
        assert_eq!(
            USERNAME_STATUS_OPTIONS,
            ["unclaimed", "claimed", "premium", "indefinite"]
        );
        assert_eq!(BARE_CLAIM_OPTIONS, ["claimed", "unavailable", "not_eligible"]);
        assert_eq!(
            TRIBE_NAME_STATUS_OPTIONS,
            ["pending", "live", "rejected", "revoked"]
        );
        assert_eq!(PLAYER_GAME_MODE_FILTER_OPTIONS, PLAYER_GAME_MODE_FILTERS);
        assert_eq!(PLAYER_GAME_TYPE_FILTER_OPTIONS, PLAYER_GAME_TYPE_FILTERS);
        assert_eq!(PLAYER_GAME_RESULT_OPTIONS, ["victory", "defeat", "incomplete"]);
        assert_eq!(PAYMENTS_PROVIDER_OPTIONS, ["steam", "stripe"]);
        assert_eq!(
            PAYMENTS_KIND_OPTIONS,
            ["currency_pack", "custom_currency", "subscription_tier"]
        );
        assert_eq!(
            PAYMENTS_HANDOFF_OPTIONS,
            ["redirect", "client_overlay", "client_secret"]
        );
        assert_eq!(
            STEAM_ORDER_RESOLUTION_OPTIONS,
            ["settled", "expired", "open", "unresolved"]
        );
    }

    #[test]
    fn admin_role_edges() {
        assert!(is_admin_role(&units("admin")));
        assert!(is_admin_role(&units("root")));
        assert!(!is_admin_role(&units("mod")));
        assert!(!is_admin_role(&units("Admin")));
        assert!(!is_admin_role(&units("ADMIN")));
        assert!(!is_admin_role(&units("")));
        assert!(!is_admin_role(&units("administrator")));
    }

    #[test]
    fn temporary_username_edges() {
        assert!(is_temporary_username(&units("TEMPORARY1234")));
        assert!(is_temporary_username(&units("TEMPORARY0000")));
        assert!(!is_temporary_username(&units("TEMPORARY123")));
        assert!(!is_temporary_username(&units("TEMPORARY12345")));
        assert!(!is_temporary_username(&units("temporary1234")));
        // No u flag: \d is ASCII 0-9 only.
        assert!(!is_temporary_username(&units("TEMPORARY\u{0661}\u{0662}\u{0663}\u{0664}")));
        assert!(!is_temporary_username(&units("TEMPORARY12 4")));
        assert!(!is_temporary_username(&units("XTEMPORARY1234")));
        assert!(!is_temporary_username(&units("TEMPORARY1234X")));
        assert!(!is_temporary_username(&units("")));
    }

    #[test]
    fn verified_username_edges() {
        assert!(is_verified_username(&units("Ninja")));
        assert!(!is_verified_username(&units("Ninja.4471")));
        assert!(!is_verified_username(&units("TEMPORARY1234")));
        assert!(!is_verified_username(&units("TEMPORARY1234.5")));
        assert!(!is_verified_username(&units("a.b.")));
        assert!(!is_verified_username(&units(".")));
        assert!(is_verified_username(&units("")));
    }

    #[test]
    fn granted_subscription_states() {
        assert!(!is_granted_subscription(None));
        assert!(is_granted_subscription(Some(SubscriptionProvider::Null)));
        assert!(!is_granted_subscription(Some(SubscriptionProvider::Missing)));
        assert!(!is_granted_subscription(Some(SubscriptionProvider::Rail(&units(
            "steam",
        )))));
    }

    #[test]
    fn run_op_dump_shapes() {
        assert_eq!(run_op(0, &[0.0])[0], 2.0);
        assert_eq!(run_op(1, &[0.0])[0], 3.0);
        assert_eq!(run_op(2, &[0.0])[0], 4.0);
        assert_eq!(run_op(3, &[0.0])[0], 3.0);
        assert_eq!(run_op(4, &[0.0])[0], 4.0);
        assert_eq!(run_op(5, &[0.0])[0], 3.0);
        assert_eq!(run_op(6, &[0.0])[0], 4.0);
        assert_eq!(run_op(7, &[0.0])[0], 3.0);
        assert_eq!(run_op(8, &[0.0])[0], 2.0);
        assert_eq!(run_op(9, &[0.0])[0], 3.0);
        assert_eq!(run_op(10, &[0.0])[0], 3.0);
        assert_eq!(run_op(11, &[0.0])[0], 4.0);
        // First ADMIN_ROLES entry round-trips as "admin".
        let d = run_op(0, &[0.0]);
        assert_eq!(d[1], 5.0);
        assert_eq!(&d[2..7], &[97.0, 100.0, 109.0, 105.0, 110.0]);
    }

    #[test]
    fn run_op_predicate_batches() {
        let mut args = vec![2.0];
        args.extend(enc_str("admin"));
        args.extend(enc_str("mod"));
        assert_eq!(run_op(12, &args), vec![2.0, 1.0, 0.0]);

        let mut args = vec![2.0];
        args.extend(enc_str("TEMPORARY1234"));
        args.extend(enc_str("TEMPORARY123"));
        assert_eq!(run_op(13, &args), vec![2.0, 1.0, 0.0]);

        let mut args = vec![2.0];
        args.extend(enc_str("Ninja"));
        args.extend(enc_str("Ninja.4471"));
        assert_eq!(run_op(14, &args), vec![2.0, 1.0, 0.0]);

        // kind 15: undefined / null / string / missing.
        let mut args = vec![4.0];
        args.push(0.0); // undefined
        args.push(2.0); // provider null
        args.extend([1.0, 5.0, 115.0, 116.0, 101.0, 97.0, 109.0]); // "steam"
        args.extend([1.0, 0.0]); // provider "" (present, empty string)
        assert_eq!(run_op(15, &args), vec![4.0, 0.0, 1.0, 0.0, 0.0]);
    }
}
