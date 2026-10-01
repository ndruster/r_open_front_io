//! Port of `src/core/ServerList.ts`.
//!
//! Server-discovery helpers: commit-identifier matching, version-aware
//! routing, and `/v/<commit>/` URL rewriting. The TS file's zod schema
//! declarations are wire-format validation at the API boundary and are
//! **not** ported — only the pure functions (the TS header itself says
//! "Pure: no window, no fetch"). Callers hand these functions zod-parsed
//! lists, so `servers` is modelled as an ordered key/value vector (JS
//! `Object.entries` insertion order) with exact-key lookup; inherited
//! `Object.prototype` property-access quirks (`servers["toString"]`) are
//! out of the domain zod guarantees.
//!
//! Faithfulness notes:
//!
//! * `isCommitLike` is `/^[0-9a-f]{7,40}$/i`. The `i` flag adds no non-ASCII
//!   matches here (canonical case folding reaches only `K`/`S` variants, not
//!   `a-f`), so ASCII hexdigit counting is exact; JS counts UTF-16 units, but
//!   any non-ASCII unit fails the class anyway.
//! * `isSiteLike` is `length <= 253` (UTF-16 units) + `/^[a-z0-9](?:[a-z0-9.-]*[a-z0-9])?$/`
//!   (no `i` flag, so uppercase fails) + `!includes("..")`.
//! * `commitsMatch` lowercases only after both operands pass `isCommitLike`
//!   (ASCII-safe), and the non-commit branch is JS `===` (case-sensitive).
//! * `ownLetterIn` compares hosts with JS `toLowerCase()` — Unicode-aware, so
//!   Rust's `str::to_lowercase` (which also special-cases the Greek final
//!   sigma and `İ`) is the faithful twin, not `to_ascii_lowercase`.
//! * `pathNamesGame` decodes the id segment with `decodeURIComponent` inside
//!   a `try`/`catch`: a malformed escape (`%`, `%f`, `%zz`, a lone
//!   continuation byte, an overlong sequence, or an encoded surrogate) falls
//!   back to the **raw** segment.
//! * `pickServerForBuild`'s `Number.isInteger` gate sends `NaN`, `±Infinity`
//!   and fractional picks to index `0`; integer picks clamp into
//!   `[0, count-1]` (`-0` is an integer and lands on `0`).
//! * The regexes are hand-transcribed: `[^/]+` and `[^/?#]+` stop at the
//!   first ASCII delimiter, `\d+` is ASCII-only (no `u` flag), and
//!   `String.replace` of `/^\/w\d+\//` rewrites only a prefix match.

/// `ServerState` — the zod enum's three values.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ServerState {
    Open,
    Draining,
    Fenced,
}

/// One `ServerEntry` (only the fields the ported functions read).
#[derive(Clone, Debug)]
pub struct ServerEntry {
    pub host: String,
    pub num_workers: f64,
    pub version: String,
    pub state: ServerState,
}

/// `ServerList.servers` as an ordered record (JS insertion order).
#[derive(Clone, Debug, Default)]
pub struct ServerList {
    pub servers: Vec<(String, ServerEntry)>,
}

impl ServerList {
    fn get(&self, letter: &str) -> Option<&ServerEntry> {
        self.servers.iter().find(|(k, _)| k == letter).map(|(_, v)| v)
    }
}

/// `isCommitLike` — `/^[0-9a-f]{7,40}$/i`.
pub fn is_commit_like(value: &str) -> bool {
    let n = value.len(); // all matching chars are ASCII; non-ASCII fails below
    if !(7..=40).contains(&n) {
        return false;
    }
    value.bytes().all(|b| b.is_ascii_hexdigit())
}

/// `isSiteLike` — length cap + hostname regex + no `..`.
pub fn is_site_like(value: &str) -> bool {
    if value.encode_utf16().count() > 253 {
        return false;
    }
    let b = value.as_bytes();
    if b.is_empty() || value.contains("..") {
        return false;
    }
    let edge = |c: u8| c.is_ascii_lowercase() || c.is_ascii_digit();
    let mid = |c: u8| edge(c) || c == b'.' || c == b'-';
    if !edge(b[0]) {
        return false;
    }
    if b.len() == 1 {
        return true;
    }
    edge(b[b.len() - 1]) && b[1..b.len() - 1].iter().all(|&c| mid(c))
}

/// `commitsMatch(a, b)` — prefix match once both are commit-shaped, else `===`.
pub fn commits_match(a: &str, b: &str) -> bool {
    if !is_commit_like(a) || !is_commit_like(b) {
        return a == b;
    }
    let x = a.to_ascii_lowercase();
    let y = b.to_ascii_lowercase();
    if x.len() <= y.len() {
        y.starts_with(&x)
    } else {
        x.starts_with(&y)
    }
}

/// `versionMatches(ownCommit, serverVersion)`.
pub fn version_matches(own_commit: &str, server_version: &str) -> bool {
    if !is_commit_like(own_commit) {
        return true;
    }
    commits_match(own_commit, server_version)
}

/// `lettersForBuild` — open / draining letters on this build, in record order.
fn letters_for_build(list: &ServerList, own_commit: &str) -> (Vec<String>, Vec<String>) {
    let mut open = Vec::new();
    let mut draining = Vec::new();
    for (letter, entry) in &list.servers {
        if !version_matches(own_commit, &entry.version) {
            continue;
        }
        match entry.state {
            ServerState::Open => open.push(letter.clone()),
            ServerState::Draining => draining.push(letter.clone()),
            ServerState::Fenced => {}
        }
    }
    (open, draining)
}

/// `servesBuild(list, letter, ownCommit)`.
pub fn serves_build(list: &ServerList, letter: &str, own_commit: &str) -> bool {
    let entry = match list.get(letter) {
        Some(e) => e,
        None => return false,
    };
    if entry.state == ServerState::Fenced {
        return false;
    }
    version_matches(own_commit, &entry.version)
}

/// `pickServerForBuild(list, ownCommit, pickIndex)` — the injected picker is
/// called with the candidate count.
pub fn pick_server_for_build(
    list: &ServerList,
    own_commit: &str,
    pick_index: &dyn Fn(f64) -> f64,
) -> Option<String> {
    let (open, draining) = letters_for_build(list, own_commit);
    let candidates = if !open.is_empty() { &open } else { &draining };
    if candidates.is_empty() {
        return None;
    }
    let count = candidates.len() as f64;
    let chosen = pick_index(count);
    let index = if chosen.is_finite() && chosen == chosen.trunc() {
        chosen.max(0.0).min(count - 1.0) as usize
    } else {
        0
    };
    Some(candidates[index].clone())
}

/// `ownLetterIn(list, ownHost, ownLetter)`.
pub fn own_letter_in(
    list: &ServerList,
    own_host: Option<&str>,
    own_letter: Option<&str>,
) -> Option<String> {
    if let Some(h) = own_host {
        if !h.is_empty() {
            let want = h.to_lowercase();
            for (letter, entry) in &list.servers {
                if entry.host.to_lowercase() == want {
                    return Some(letter.clone());
                }
            }
            return None;
        }
    }
    let letter = own_letter?;
    list.get(letter).map(|_| letter.to_string())
}

/// `stripVersionPrefix(pathname)` — `/^\/v\/([^/]+)(\/|$)/`.
pub fn strip_version_prefix(pathname: &str) -> (Option<String>, String) {
    let after = match pathname.strip_prefix("/v/") {
        Some(a) => a,
        None => return (None, pathname.to_string()),
    };
    let end = after.find('/').unwrap_or(after.len());
    if end == 0 {
        return (None, pathname.to_string());
    }
    let commit = after[..end].to_string();
    let rest = if end < after.len() {
        &after[end + 1..]
    } else {
        ""
    };
    (Some(commit), format!("/{rest}"))
}

/// `shortCommit(commit)` — the first 7 lowercase hex characters, or the
/// value untouched when it names no commit.
pub fn short_commit(commit: &str) -> String {
    if !is_commit_like(commit) {
        return commit.to_string();
    }
    commit.to_ascii_lowercase().chars().take(7).collect()
}

/// `path.replace(/^\/w\d+\//, "/")` — drop a worker prefix.
fn strip_worker_prefix(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("/w") {
        let b = rest.as_bytes();
        let mut i = 0;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        if i > 0 && i < b.len() && b[i] == b'/' {
            return format!("/{}", &rest[i + 1..]);
        }
    }
    path.to_string()
}

/// `versionedPath(commit, pathname, search)` — `null` when the page already
/// lives under `/v/<commit>/`.
pub fn versioned_path(commit: &str, pathname: &str, search: &str) -> Option<String> {
    let (current, path) = strip_version_prefix(pathname);
    if let Some(cur) = &current {
        if commits_match(cur, commit) {
            return None;
        }
    }
    let bare = strip_worker_prefix(&path);
    Some(format!("/v/{}{bare}{search}", short_commit(commit)))
}

/// `decodeURIComponent` with JS semantics: `None` models the `URIError` a
/// malformed escape raises (caller falls back to the raw segment).
fn decode_uri_component(s: &str) -> Option<String> {
    fn hex_byte(b: &[u8], p: usize) -> Option<u8> {
        if p + 2 >= b.len() || b[p] != b'%' {
            return None;
        }
        let h1 = (b[p + 1] as char).to_digit(16)?;
        let h2 = (b[p + 2] as char).to_digit(16)?;
        Some((h1 * 16 + h2) as u8)
    }
    let cont = |v: u8| (0x80..=0xBF).contains(&v);
    let b = s.as_bytes();
    let mut out = String::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] != b'%' {
            let ch = s[i..].chars().next()?;
            out.push(ch);
            i += ch.len_utf8();
            continue;
        }
        let v = hex_byte(b, i)?;
        i += 3;
        if v < 0x80 {
            out.push(v as char);
            continue;
        }
        if v < 0xC0 {
            return None; // continuation byte at a sequence start
        }
        if v < 0xE0 {
            let w = hex_byte(b, i)?;
            i += 3;
            if !cont(w) {
                return None;
            }
            let cp = ((v as u32 & 0x1F) << 6) | (w as u32 & 0x3F);
            if cp < 0x80 {
                return None; // overlong
            }
            out.push(char::from_u32(cp)?);
            continue;
        }
        if v < 0xF0 {
            let w = hex_byte(b, i)?;
            i += 3;
            let q = hex_byte(b, i)?;
            i += 3;
            if !cont(w) || !cont(q) {
                return None;
            }
            let cp = ((v as u32 & 0xF) << 12) | ((w as u32 & 0x3F) << 6) | (q as u32 & 0x3F);
            if cp < 0x800 || (0xD800..=0xDFFF).contains(&cp) {
                return None; // overlong or encoded surrogate
            }
            out.push(char::from_u32(cp)?);
            continue;
        }
        if v < 0xF8 {
            let w = hex_byte(b, i)?;
            i += 3;
            let q = hex_byte(b, i)?;
            i += 3;
            let r = hex_byte(b, i)?;
            i += 3;
            if !cont(w) || !cont(q) || !cont(r) {
                return None;
            }
            let cp = ((v as u32 & 0x7) << 18)
                | ((w as u32 & 0x3F) << 12)
                | ((q as u32 & 0x3F) << 6)
                | (r as u32 & 0x3F);
            if !(0x10000..=0x10FFFF).contains(&cp) {
                return None;
            }
            out.push(char::from_u32(cp)?);
            continue;
        }
        return None;
    }
    Some(out)
}

/// The `([^/?#]+)` capture of `/^(?:\/w\d+)?\/game\//`, if the prefix matches.
fn game_path_id(version_free_path: &str) -> Option<&str> {
    let mut s = version_free_path;
    if let Some(rest) = s.strip_prefix("/w") {
        let b = rest.as_bytes();
        let mut i = 0;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        if i > 0 && i < b.len() && b[i] == b'/' {
            s = &rest[i..];
        }
    }
    let after = s.strip_prefix("/game/")?;
    let end = after.find(['/', '?', '#']).unwrap_or(after.len());
    if end == 0 {
        return None;
    }
    Some(&after[..end])
}

/// `pathNamesGame(versionFreePath, gameID)` — module-private in TS, exposed
/// here because the probe and vectors exercise it directly.
pub fn path_names_game(version_free_path: &str, game_id: &str) -> bool {
    let seg = match game_path_id(version_free_path) {
        Some(s) => s,
        None => return false,
    };
    let decoded = decode_uri_component(seg).unwrap_or_else(|| seg.to_string());
    decoded == game_id
}

/// `versionedPathForGame(...)`, `spectator` defaulting to `false` in TS.
pub fn versioned_path_for_game(
    own_commit: &str,
    game_version: Option<&str>,
    game_id: &str,
    game_version_free_path: &str,
    pathname: &str,
    search: &str,
    spectator: bool,
) -> Option<String> {
    let gv = game_version?;
    if version_matches(own_commit, gv) {
        return None;
    }
    let (current, path) = strip_version_prefix(pathname);
    if let Some(cur) = &current {
        if commits_match(cur, gv) {
            return None;
        }
    }
    if path_names_game(&path, game_id) {
        versioned_path(gv, pathname, search)
    } else {
        versioned_path(
            gv,
            game_version_free_path,
            if spectator { "?spectate" } else { "" },
        )
    }
}

// ---------------------------------------------------------------- vectors op
//
// A flat `f64` token runner shared by the golden replay and the wasm probe.
// Strings cross as `[len, u0, .. u(len-1)]` (UTF-16 code units), an optional
// string as `[0]` / `[1, string]`, a list as `[n, (letter, host, numWorkers,
// version, state) * n]` with state `0=open, 1=draining, 2=fenced`. Results
// encode booleans as `0/1` and `string | null` as `[-1]` or the string.

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
    fn opt_string(&mut self) -> Option<String> {
        if self.f() != 0.0 {
            Some(self.string())
        } else {
            None
        }
    }
    fn list(&mut self) -> ServerList {
        let n = self.u();
        ServerList {
            servers: (0..n)
                .map(|_| {
                    let letter = self.string();
                    let host = self.string();
                    let num_workers = self.f();
                    let version = self.string();
                    let state = match self.u() {
                        0 => ServerState::Open,
                        1 => ServerState::Draining,
                        _ => ServerState::Fenced,
                    };
                    (letter, ServerEntry { host, num_workers, version, state })
                })
                .collect(),
        }
    }
}

fn push_string(out: &mut Vec<f64>, s: &str) {
    let units: Vec<u16> = s.encode_utf16().collect();
    out.push(units.len() as f64);
    out.extend(units.iter().map(|&u| u as f64));
}

fn push_opt_string(out: &mut Vec<f64>, s: Option<&str>) {
    match s {
        None => out.push(-1.0),
        Some(v) => push_string(out, v),
    }
}

/// `kind`: 0 isCommitLike, 1 isSiteLike, 2 commitsMatch, 3 versionMatches,
/// 4 servesBuild, 5 pickServerForBuild (pick kind 0 const arg, 1 `n-1`, 2 `n`,
/// 3 `-1`), 6 ownLetterIn, 7 stripVersionPrefix, 8 shortCommit, 9
/// versionedPath, 10 pathNamesGame, 11 versionedPathForGame.
pub fn run_op(kind: u8, args: &[f64]) -> Vec<f64> {
    let mut c = Cur(args, 0);
    let mut out = Vec::new();
    match kind {
        0 => {
            let s = c.string();
            out.push(f64::from(is_commit_like(&s)));
        }
        1 => {
            let s = c.string();
            out.push(f64::from(is_site_like(&s)));
        }
        2 => {
            let a = c.string();
            let b = c.string();
            out.push(f64::from(commits_match(&a, &b)));
        }
        3 => {
            let own = c.string();
            let ver = c.string();
            out.push(f64::from(version_matches(&own, &ver)));
        }
        4 => {
            let l = c.list();
            let letter = c.string();
            let own = c.string();
            out.push(f64::from(serves_build(&l, &letter, &own)));
        }
        5 => {
            let l = c.list();
            let own = c.string();
            let pk = c.u();
            let pa = c.f();
            let got = pick_server_for_build(&l, &own, &|n| match pk {
                0 => pa,
                1 => n - 1.0,
                2 => n,
                _ => -1.0,
            });
            push_opt_string(&mut out, got.as_deref());
        }
        6 => {
            let l = c.list();
            let host = c.opt_string();
            let letter = c.opt_string();
            let got = own_letter_in(&l, host.as_deref(), letter.as_deref());
            push_opt_string(&mut out, got.as_deref());
        }
        7 => {
            let p = c.string();
            let (commit, path) = strip_version_prefix(&p);
            push_opt_string(&mut out, commit.as_deref());
            push_string(&mut out, &path);
        }
        8 => {
            let s = c.string();
            push_string(&mut out, &short_commit(&s));
        }
        9 => {
            let commit = c.string();
            let pathname = c.string();
            let search = c.string();
            let got = versioned_path(&commit, &pathname, &search);
            push_opt_string(&mut out, got.as_deref());
        }
        10 => {
            let p = c.string();
            let id = c.string();
            out.push(f64::from(path_names_game(&p, &id)));
        }
        _ => {
            let own = c.string();
            let gv = c.opt_string();
            let gid = c.string();
            let vfp = c.string();
            let pathname = c.string();
            let search = c.string();
            let spectator = c.f() != 0.0;
            let got = versioned_path_for_game(
                &own,
                gv.as_deref(),
                &gid,
                &vfp,
                &pathname,
                &search,
                spectator,
            );
            push_opt_string(&mut out, got.as_deref());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const C1: &str = "a1b2c3d4e5f60718293a4b5c6d7e8f9012345678";
    const C7: &str = "a1b2c3d";

    fn list1() -> ServerList {
        let e = |host: &str, version: &str, state: ServerState| ServerEntry {
            host: host.to_string(),
            num_workers: 1.0,
            version: version.to_string(),
            state,
        };
        ServerList {
            servers: vec![
                ("a".to_string(), e("a.example.com", C1, ServerState::Open)),
                ("b".to_string(), e("B.EXAMPLE.COM", "deadbeef1234567890abcdef1234567890abcd", ServerState::Draining)),
                ("c".to_string(), e("c.example.com", "00112233445566778899aabbccddeeff00112233", ServerState::Fenced)),
            ],
        }
    }

    #[test]
    fn commit_like_edges() {
        assert!(is_commit_like(C7));
        assert!(is_commit_like(C1));
        assert!(is_commit_like("ABCDEF0"));
        assert!(!is_commit_like("abcdef")); // 6
        assert!(!is_commit_like(&format!("{C1}0"))); // 41
        assert!(!is_commit_like("abcdefg"));
        assert!(!is_commit_like("0x12345"));
        assert!(!is_commit_like(""));
    }

    #[test]
    fn site_like_edges() {
        assert!(is_site_like("a"));
        assert!(is_site_like("a.b.c"));
        assert!(is_site_like("a-b"));
        assert!(!is_site_like("a..b"));
        assert!(!is_site_like("a-"));
        assert!(!is_site_like("-a"));
        assert!(!is_site_like("A"));
        assert!(!is_site_like("a_b"));
        assert!(!is_site_like(&"x".repeat(254)));
        assert!(is_site_like(&"x".repeat(253)));
    }

    #[test]
    fn commits_match_prefix_and_identity() {
        assert!(commits_match(C7, C1));
        assert!(commits_match(C1, C7));
        assert!(commits_match("A1B2C3D", C1));
        assert!(!commits_match("a1b2c3e", C1));
        // Non-commit values only match themselves, case-sensitively.
        assert!(commits_match("DEV", "DEV"));
        assert!(!commits_match("DEV", "dev"));
        assert!(!commits_match(C7, "a1b2c3"));
    }

    #[test]
    fn version_matches_unlabeled_builds() {
        assert!(version_matches("DEV", C1));
        assert!(version_matches(C7, C1));
        assert!(!version_matches(C1, "deadbeef1234567890abcdef1234567890abcd"));
    }

    #[test]
    fn pick_clamps_and_falls_back() {
        let l = list1();
        let pick = |v: f64| pick_server_for_build(&l, C1, &|_| v);
        assert_eq!(pick(0.0).as_deref(), Some("a"));
        assert_eq!(pick(5.0).as_deref(), Some("a")); // open=[a], clamped
        assert_eq!(pick(f64::NAN).as_deref(), Some("a"));
        assert_eq!(pick(0.5).as_deref(), Some("a"));
        assert_eq!(pick(-3.0).as_deref(), Some("a"));
        // draining fallback when no open server matches
        assert_eq!(
            pick_server_for_build(&l, "deadbeef1234567890abcdef1234567890abcd", &|_| 0.0).as_deref(),
            Some("b")
        );
    }

    #[test]
    fn pick_clamp_open_only() {
        let l = list1();
        // own matches C1: open = [a] only, so any integer clamps to "a".
        assert_eq!(pick_server_for_build(&l, C1, &|_| 9.0).as_deref(), Some("a"));
    }

    #[test]
    fn own_letter_host_decides() {
        let l = list1();
        assert_eq!(own_letter_in(&l, Some("A.EXAMPLE.COM"), Some("b")).as_deref(), Some("a"));
        assert_eq!(own_letter_in(&l, Some("nope"), Some("b")), None);
        assert_eq!(own_letter_in(&l, Some(""), Some("b")).as_deref(), Some("b"));
        assert_eq!(own_letter_in(&l, None, Some("z")), None);
        assert_eq!(own_letter_in(&l, None, None), None);
    }

    #[test]
    fn strip_version_prefix_edges() {
        assert_eq!(
            strip_version_prefix(&format!("/v/{C1}/game/5")),
            (Some(C1.to_string()), "/game/5".to_string())
        );
        assert_eq!(
            strip_version_prefix(&format!("/v/{C7}")),
            (Some(C7.to_string()), "/".to_string())
        );
        assert_eq!(strip_version_prefix("/v//x"), (None, "/v//x".to_string()));
        assert_eq!(strip_version_prefix("/v/"), (None, "/v/".to_string()));
        assert_eq!(strip_version_prefix("/game/5"), (None, "/game/5".to_string()));
    }

    #[test]
    fn worker_prefix_strip_edges() {
        assert_eq!(strip_worker_prefix("/w12/game/5"), "/game/5");
        assert_eq!(strip_worker_prefix("/w12x/game/5"), "/w12x/game/5");
        assert_eq!(strip_worker_prefix("/w/game/5"), "/w/game/5");
        assert_eq!(strip_worker_prefix("/w1/2/"), "/2/");
    }

    #[test]
    fn path_names_game_decode() {
        assert!(path_names_game("/game/abc", "abc"));
        assert!(path_names_game("/w12/game/abc", "abc"));
        assert!(path_names_game("/game/abc?x", "abc"));
        assert!(path_names_game("/game/abc/def", "abc"));
        assert!(!path_names_game("/game/", "abc"));
        assert!(!path_names_game("/GAME/abc", "abc"));
        assert!(!path_names_game("/w/game/abc", "abc"));
        assert!(path_names_game("/game/%41", "A"));
        assert!(!path_names_game("/game/%41", "%41"));
        // malformed escapes fall back to the raw segment
        assert!(path_names_game("/game/%zz", "%zz"));
        assert!(path_names_game("/game/%", "%"));
        assert!(path_names_game("/game/%C0%80", "%C0%80"));
        assert!(path_names_game("/game/%E4%B8%AD", "中"));
        assert!(path_names_game("/game/%F0%9F%98%80", "😀"));
        assert!(path_names_game("/game/%2F", "/"));
    }

    #[test]
    fn versioned_path_loop_guard() {
        assert!(versioned_path(C1, &format!("/v/{C7}/game/5"), "").is_none());
        assert_eq!(
            versioned_path(C1, "/w12/game/5", "?lobby").as_deref(),
            Some("/v/a1b2c3d/game/5?lobby")
        );
        assert_eq!(versioned_path("DEV", "/game/5", "").as_deref(), Some("/v/DEV/game/5"));
    }

    #[test]
    fn versioned_path_for_game_rules() {
        let c2 = "deadbeef1234567890abcdef1234567890abcd";
        assert_eq!(
            versioned_path_for_game("DEV", Some(c2), "5", "/game/5", "/game/5", "", false),
            None
        );
        assert_eq!(
            versioned_path_for_game(C1, Some(c2), "5", "/game/5", "/game/5", "", false).as_deref(),
            Some("/v/deadbee/game/5")
        );
        assert_eq!(
            versioned_path_for_game(C1, Some(c2), "5", "/game/5", "/v/deadbee/game/5", "", false),
            None
        );
        assert_eq!(
            versioned_path_for_game(C1, Some(c2), "5", "/game/5", "/game/OTHER", "", true)
                .as_deref(),
            Some("/v/deadbee/game/5?spectate")
        );
    }
}
