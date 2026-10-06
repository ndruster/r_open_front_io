//! Port of `src/client/GameVersion.ts` — the build-label composer
//! (`composeGameVersion` / `taggedGameVersion`). The env-reading functions
//! (`currentGameVersion` / `renderNavVersion` / `currentGitCommit`) stay in
//! TS: they read `resources/version.txt`, `ClientEnv` and the DOM.
//!
//! Faithfulness notes (quirk list):
//!
//! * `VERSION_RE = /^v?\d+\.\d+\.\d+/` is UNANCHORED at the tail (no `$`):
//!   `"v1.2.3-beta"` passes; `\d` without the `u` flag is ASCII `[0-9]`
//!   only (Arabic-Indic digits fail).
//! * `SHA_RE = /^[0-9a-f]{7,40}$/i` — the length is in UTF-16 units but the
//!   charset is ASCII, so a byte gate is exact on the pass path; `$` without
//!   `m` is end-of-input (a trailing `\n` fails).
//! * The `v` prefix gate is `trimmed.startsWith("v")` — lowercase only:
//!   `"V1.2.3"` trims, fails `^v?` at position 0? no — `^v?` makes the v
//!   OPTIONAL, so `"V1.2.3"` fails the regex (V is not a digit) and gets the
//!   `v` PREFIX ADDED → `"vV1.2.3"` while the regex test itself is on the
//!   TRIMMED value, not `withV`.
//! * `commit.slice(0, 7).toLowerCase()` only runs on SHA_RE passes (pure
//!   ASCII hex), so the slice is byte-safe.
//! * The placeholder `"x.xx.xx"` fails VERSION_RE (x is not a digit) and
//!   falls to the commit branch — the whole point of the module.

use crate::js_json::{push_str, read_str};

/// `SHORT_COMMIT_LENGTH`.
pub const SHORT_COMMIT_LENGTH: usize = 7;

/// JS `String.prototype.trim`: the WhiteSpace ∪ LineTerminator set (U+0085
/// is NOT in it, U+FEFF IS — unlike Rust's `str::trim`).
fn js_trim(s: &str) -> &str {
    s.trim_matches(|c: char| {
        matches!(
            c,
            '\t' | '\n'
                | '\u{b}'
                | '\u{c}'
                | '\r'
                | ' '
                | '\u{a0}'
                | '\u{1680}'
                | '\u{2000}'..='\u{200a}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202f}'
                | '\u{205f}'
                | '\u{3000}'
                | '\u{feff}'
        )
    })
}

/// `/^v?\d+\.\d+\.\d+/` test (unanchored tail, ASCII `\d`, each run one or
/// more digits).
fn version_re_test(s: &str) -> bool {
    let b = s.as_bytes();
    let mut p = 0usize;
    if p < b.len() && b[p] == b'v' {
        p += 1;
    }
    let d = p;
    while p < b.len() && b[p].is_ascii_digit() {
        p += 1;
    }
    if p == d {
        return false;
    }
    if p >= b.len() || b[p] != b'.' {
        return false;
    }
    p += 1;
    let d = p;
    while p < b.len() && b[p].is_ascii_digit() {
        p += 1;
    }
    if p == d {
        return false;
    }
    if p >= b.len() || b[p] != b'.' {
        return false;
    }
    p += 1;
    p < b.len() && b[p].is_ascii_digit()
}

/// `/^[0-9a-f]{7,40}$/i` test.
fn sha_re_test(s: &str) -> bool {
    let n = s.encode_utf16().count();
    (7..=40).contains(&n)
        && s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b) || (b'A'..=b'F').contains(&b))
}

/// `composeGameVersion(rawVersion, gitCommit)`.
pub fn compose_game_version(raw_version: &str, git_commit: &str) -> String {
    let trimmed = js_trim(raw_version);
    let with_v = if trimmed.starts_with('v') {
        trimmed.to_string()
    } else {
        format!("v{trimmed}")
    };
    if version_re_test(trimmed) {
        return with_v;
    }
    let commit = js_trim(git_commit);
    if sha_re_test(commit) {
        return commit[..SHORT_COMMIT_LENGTH].to_lowercase();
    }
    if !commit.is_empty() {
        return commit.to_string();
    }
    with_v
}

/// `taggedGameVersion(rawVersion)`.
pub fn tagged_game_version(raw_version: &str) -> String {
    let trimmed = js_trim(raw_version);
    if trimmed.starts_with('v') {
        trimmed.to_string()
    } else {
        format!("v{trimmed}")
    }
}

// ---------------------------------------------------------------- vectors op
//
// Strings cross as `[len, u0, ..]` UTF-16 units.
//
// kind 0: [...str(rawVersion), ...str(gitCommit)] -> [...str(label)]
// kind 1: [n, (str)*n] -> [n, (str)*n]            taggedGameVersion batch

/// Flat-token runner shared by the golden replay and the wasm probe.
pub fn run_op(kind: u8, args: &[f64]) -> Vec<f64> {
    let mut out = Vec::new();
    match kind {
        0 => {
            let mut i = 0usize;
            let raw = read_str(args, &mut i);
            let commit = read_str(args, &mut i);
            let label = compose_game_version(&raw, &commit);
            push_str(&mut out, &label);
        }
        1 => {
            let n = args[0] as usize;
            let mut i = 1usize;
            out.push(n as f64);
            for _ in 0..n {
                let s = read_str(args, &mut i);
                let v = tagged_game_version(&s);
                push_str(&mut out, &v);
            }
        }
        k => unreachable!("game_version: unknown op kind {k}"),
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tagged_passes() {
        assert_eq!(compose_game_version("v1.2.3", "bf739f8"), "v1.2.3");
        assert_eq!(compose_game_version("1.2.3-beta", "x"), "v1.2.3-beta");
        assert_eq!(compose_game_version("0.33.18", "x"), "v0.33.18");
    }

    #[test]
    fn placeholder_falls_to_commit() {
        // The shipping placeholder fails VERSION_RE; a real sha wins.
        assert_eq!(compose_game_version("x.xx.xx", "bf739f8de"), "bf739f8");
        assert_eq!(
            compose_game_version("x.xx.xx", "  BF739F8DE  "),
            "bf739f8"
        );
        // Not a sha but says more than the placeholder.
        assert_eq!(compose_game_version("x.xx.xx", "DEV"), "DEV");
        assert_eq!(compose_game_version("x.xx.xx", "desktop"), "desktop");
        // Nothing at all: whatever the file held.
        assert_eq!(compose_game_version("x.xx.xx", ""), "vx.xx.xx");
    }

    #[test]
    fn regex_edges() {
        // Uppercase V fails the regex, gets the prefix added.
        assert_eq!(compose_game_version("V1.2.3", ""), "vV1.2.3");
        // SHA too short / too long / non-hex.
        assert_eq!(compose_game_version("x", "abcdef"), "abcdef"); // 6 chars
        assert_eq!(compose_game_version("x", "g".repeat(8).as_str()), "g".repeat(8));
        // Trailing newline fails `$`.
        assert_eq!(compose_game_version("x", "bf739f8\n"), "bf739f8"); // trim eats \n
        assert_eq!(compose_game_version("x", "bf739f8x"), "bf739f8x");
        assert_eq!(tagged_game_version("  1.2.3 "), "v1.2.3");
        assert_eq!(tagged_game_version("v9"), "v9");
    }
}
