//! Port of `src/client/ClientPlatform.ts` — `clientPlatform()`. The two
//! host-bound predicates (`isDesktopShell()` reading
//! `window.openfrontDesktop`, and `crazyGamesSDK.isOnCrazyGames()`, an
//! iframe probe) are facades (precedent: the `gms_` DesktopShell
//! predicates). The capture scripts `globalThis.window` behind an
//! accessor-counting getter, monkey-patches the SDK singleton and
//! re-imports this module per scenario, so the golden pins the REAL
//! short-circuit order through observable counts.
//!
//! Faithfulness notes (quirk list):
//!
//! * `isDesktopShell()` is called FIRST and unconditionally — its truthy
//!   return short-circuits to `"steam"` and the second gate's `typeof
//!   window` read never executes. With `window` scripted behind a getter,
//!   `isDesktopShell` itself triggers exactly TWO `window` reads (the
//!   `typeof` plus the `.openfrontDesktop` GetValue), so the steam path
//!   dumps 2 reads and 0 SDK calls.
//! * The second gate is `typeof window !== "undefined" && …` — with
//!   `window` absent the `&&` short-circuits and `isOnCrazyGames` is NEVER
//!   called (0 reads, 0 SDK calls, `"web"`). With `window` present and the
//!   shell false, the gate's own `typeof` adds a THIRD read and the SDK
//!   call happens (3 reads, 1 SDK call).
//! * Contradictory scripts (desktop true with no window) are outside the
//!   capture domain: `isDesktopShell()` can only return true through the
//!   window object, so `desktop = 1` implies `window_present = 1`.
//! * The Node capture has no `window` unless scripted, which pins the
//!   `typeof` gate against the global property; the Rust twin models the
//!   same three counts from the scripted inputs.

/// Result tokens: 0 = "steam", 1 = "crazygames", 2 = "web".
pub const RESULT_STEAM: f64 = 0.0;
pub const RESULT_CRAZYGAMES: f64 = 1.0;
pub const RESULT_WEB: f64 = 2.0;

/// `clientPlatform()` over the scripted facade returns; pushes
/// `(result, window_reads, sdk_calls)` onto `out`. `window_reads` counts the
/// global `window` GetValue events (2 = shell short-circuit, 3 = the second
/// gate also read `typeof window`, 0 = window absent), `sdk_calls` counts
/// the `isOnCrazyGames()` invocations.
pub fn client_platform(
    desktop_ret: bool,
    window_present: bool,
    cg_ret: bool,
    out: &mut Vec<f64>,
) {
    let (result, reads, calls) = if desktop_ret {
        (RESULT_STEAM, 2.0, 0.0)
    } else if window_present {
        (
            if cg_ret { RESULT_CRAZYGAMES } else { RESULT_WEB },
            3.0,
            1.0,
        )
    } else {
        (RESULT_WEB, 0.0, 0.0)
    };
    out.push(result);
    out.push(reads);
    out.push(calls);
}

// ---------------------------------------------------------------- vectors op
//
// kind 0: [n, (desktop_ret 0|1, window_present 0|1, cg_ret 0|1)*n] ->
//         [n, (result 0|1|2, window_reads 0|2|3, sdk_calls 0|1)*n] — one
//         scripted `clientPlatform()` call per triple.

pub fn run_op(kind: u8, args: &[f64]) -> Vec<f64> {
    let mut out = Vec::new();
    let mut i = 0usize;
    match kind {
        0 => {
            let n = args[i] as usize;
            i += 1;
            out.push(n as f64);
            for _ in 0..n {
                let desktop = args[i] != 0.0;
                let window = args[i + 1] != 0.0;
                let cg = args[i + 2] != 0.0;
                i += 3;
                client_platform(desktop, window, cg, &mut out);
            }
        }
        k => unreachable!("client_platform: unknown op kind {k}"),
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(d: bool, w: bool, c: bool) -> Vec<f64> {
        let mut o = Vec::new();
        client_platform(d, w, c, &mut o);
        o
    }

    #[test]
    fn steam_short_circuits() {
        // isDesktopShell true -> the second gate's typeof window never
        // runs: only isDesktopShell's own two reads, no SDK call (even
        // with the other inputs scripted true).
        assert_eq!(call(true, true, true), vec![RESULT_STEAM, 2.0, 0.0]);
    }

    #[test]
    fn no_window_skips_the_sdk_call() {
        assert_eq!(call(false, false, true), vec![RESULT_WEB, 0.0, 0.0]);
    }

    #[test]
    fn window_present_consults_the_sdk() {
        assert_eq!(call(false, true, true), vec![RESULT_CRAZYGAMES, 3.0, 1.0]);
        assert_eq!(call(false, true, false), vec![RESULT_WEB, 3.0, 1.0]);
    }

    #[test]
    fn batch_runner() {
        let r = run_op(0, &[2.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0]);
        assert_eq!(&r[..7], &[2.0, RESULT_STEAM, 2.0, 0.0, RESULT_WEB, 0.0, 0.0]);
    }
}
