//! Port of `src/client/VersionedReplay.ts` — the canonical replay-shell URL
//! builder and the replay-host detector. Two pure string functions.
//!
//! Faithfulness notes:
//!
//! * `versionedReplayUrl` gates `audience === "" || audience === "localhost"`
//!   (STRICT equality — `"Localhost"` builds a URL), then template-concats
//!   `https://replay.${audience}/${gameID}`. `gameID` is a branded string in
//!   TS; the capture crosses it as a plain string. A non-string gameID would
//!   stringify — outside the typed domain, not modelled.
//! * `isReplayShellHost` is `hostname.startsWith("replay.")` — case
//!   sensitive, no trim.

use crate::js_json::{push_val, read_val, JsVal};

/// `versionedReplayUrl(audience, gameID)` — null on the dev audiences.
pub fn versioned_replay_url(audience: &str, game_id: &str) -> Option<String> {
    if audience.is_empty() || audience == "localhost" {
        return None;
    }
    Some(format!("https://replay.{audience}/{game_id}"))
}

/// `isReplayShellHost(hostname)`.
pub fn is_replay_shell_host(hostname: &str) -> bool {
    hostname.starts_with("replay.")
}

// ---------------------------------------------------------------- vectors op
//
// kind 0: [...codec(audience), ...codec(gameID)] -> [...codec(url|null)]
// kind 1: [n, (codec str)*n] -> [n, (0/1)*n]     isReplayShellHost batch

/// Flat-token runner shared by the golden replay and the wasm probe.
pub fn run_op(kind: u8, args: &[f64]) -> Vec<f64> {
    let mut out = Vec::new();
    match kind {
        0 => {
            let mut i = 0usize;
            let JsVal::Str(audience) = read_val(args, &mut i) else {
                unreachable!("versioned_replay: audience must be a string");
            };
            let JsVal::Str(game_id) = read_val(args, &mut i) else {
                unreachable!("versioned_replay: gameID must be a string");
            };
            let res = match versioned_replay_url(&audience, &game_id) {
                Some(u) => JsVal::Str(u),
                None => JsVal::Null,
            };
            push_val(&mut out, &res);
        }
        1 => {
            let n = args[0] as usize;
            let mut i = 1usize;
            out.push(n as f64);
            for _ in 0..n {
                let JsVal::Str(h) = read_val(args, &mut i) else {
                    unreachable!("versioned_replay: hostname must be a string");
                };
                out.push(if is_replay_shell_host(&h) { 1.0 } else { 0.0 });
            }
        }
        k => unreachable!("versioned_replay: unknown op kind {k}"),
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_gates() {
        assert_eq!(versioned_replay_url("", "g1"), None);
        assert_eq!(versioned_replay_url("localhost", "g1"), None);
        assert_eq!(
            versioned_replay_url("Localhost", "g1").as_deref(),
            Some("https://replay.Localhost/g1")
        );
        assert_eq!(
            versioned_replay_url("openfront.io", "g1").as_deref(),
            Some("https://replay.openfront.io/g1")
        );
        assert!(is_replay_shell_host("replay.openfront.io"));
        assert!(!is_replay_shell_host("Replay.openfront.io"));
        assert!(!is_replay_shell_host("areplay.x"));
    }
}
