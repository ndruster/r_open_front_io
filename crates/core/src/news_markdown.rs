//! Port of `src/client/NewsMarkdown.ts` — `normalizeNewsMarkdown`, the
//! four-`.replace` chain. The S12 exclusion (lookbehind unsupported by the
//! Rust `regex` crate) is void: this module hand-implements exactly the
//! regex subset the four literals need (no general engine), pinned against
//! V8 execution. All string ops run on UTF-16 code units so `.` / `\w` /
//! character classes match JS's unit semantics.
//!
//! Faithfulness notes (quirk list, all V8-pinned):
//!
//! * Regex 1 `/^([^\-*\s].*?) \*\*(.+?)\*\*$/gm` runs per line (`^`/`$`
//!   multiline anchors over \n, \r, U+2028, U+2029; `.` never crosses a
//!   terminator). Both quantifiers are LAZY: the first split position i
//!   whose " **" prefix is followed by a line-final "**" wins, and the
//!   content `$2` is the shortest run ending exactly at the line-final `**`
//!   — e.g. "A **b** **c**" -> `$1`="A", `$2`="b** **c"; "A **b** **c"
//!   (no trailing `**`) does not match at all.
//! * Regex 2 `/(?<!\()\b…pull\/(\d+)\b/g`: the negative lookbehind is a
//!   plain "previous unit is not `(`" test (string start passes); the
//!   leading `\b` requires the unit before `h` to be non-word; the trailing
//!   `\b` after the greedy `\d+` can NEVER be rescued by shrinking (any
//!   shorter digit run still butts against a word unit), so "…/pull/123abc"
//!   does not match while "…/pull/123.4" matches "123".
//! * Regex 3 `compare/([\w.-]+)\b`: the capture's last unit may be `.` /
//!   `-` (non-word), in which case `\b` demands a WORD unit after it; the
//!   greedy run backtracks unit by unit until the wordness flips — so
//!   "…/compare/abc." captures "abc" (the dot survives outside the match)
//!   and "…/compare/-" / "…/compare/." never match.
//! * Regex 4 `/(^|[^\w/[`])@([a-z\d](?:[a-z\d-]{0,37}[a-z\d])?)(?![\w-])/gim`:
//!   the `i` flag widens the user-name classes to upper case (the
//!   replacement keeps the ORIGINAL case); the prefix group consumes its
//!   one unit, so with `g` two mentions sharing a separator ("@a@b") let
//!   the second one fail — lastIndex starts on the consumed '@'. The
//!   username is the LONGEST valid length ≤ 39 whose final unit is
//!   alphanumeric and whose lookahead unit is neither word nor `-`; a 40
//!   alphanumeric run has no valid length (every prefix butts against a
//!   letter) and does not match at all. A line-start (`^`) prefix is tried
//!   before the one-unit class at the same position; `\n` itself qualifies
//!   as a prefix unit.
//! * The four replaces chain strictly in source order over the full string.

use crate::player_name::is_js_space;
use crate::js_json::{read_str, push_str};

/// The JS `\w` set restricted to UTF-16 units (ASCII-only in practice).
fn is_word_u(u: u16) -> bool {
    (0x30..=0x39).contains(&u) || (0x41..=0x5A).contains(&u) || (0x61..=0x7A).contains(&u) || u == 0x5F
}

/// JS line terminators for the `m` flag anchors.
fn is_line_term_u(u: u16) -> bool {
    u == 0x0A || u == 0x0D || u == 0x2028 || u == 0x2029
}

/// `[a-z\d]` under the `i` flag.
fn is_alnum_i(u: u16) -> bool {
    (0x30..=0x39).contains(&u) || (0x41..=0x5A).contains(&u) || (0x61..=0x7A).contains(&u)
}

/// `[a-z\d-]` under the `i` flag (the username middle class).
fn is_uname_mid_u(u: u16) -> bool {
    is_alnum_i(u) || u == 0x2D
}

/// The JS `\s` set as UTF-16 units (delegates to the shared char version).
fn is_space_u(u: u16) -> bool {
    char::from_u32(u as u32).is_some_and(is_js_space)
}

fn units(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

fn from_units(u: &[u16]) -> String {
    String::from_utf16_lossy(u)
}

/// Append an ASCII literal to a UTF-16 buffer.
fn push_ascii(out: &mut Vec<u16>, s: &str) {
    debug_assert!(s.is_ascii());
    out.extend(s.bytes().map(u16::from));
}

/// Regex 1: per-line bold-header -> "## " conversion.
fn replace_hdr(l: &[u16]) -> Vec<u16> {
    let mut out: Vec<u16> = Vec::with_capacity(l.len());
    let mut s = 0usize;
    loop {
        // one line = [s, e) up to the next terminator
        let e = l[s..]
            .iter()
            .position(|&u| is_line_term_u(u))
            .map(|p| s + p)
            .unwrap_or(l.len());
        // ^([^\-*\s].*?) \*\*(.+?)\*\*$ — lazy split search. The closing
        // "**" is pinned at the line end by $, so the line must end with
        // two stars; the first " **" whose tail leaves .+? >= 1 unit wins.
        let mut done = false;
        if e - s >= 7 {
            let first = l[s];
            if first != b'-' as u16 && first != b'*' as u16 && !is_space_u(first)
                && l[e - 2] == b'*' as u16
                && l[e - 1] == b'*' as u16
            {
                for i in (s + 1)..(e - 5) {
                    // " **" literal at i..i+3, content [i+3, e-2)
                    if l[i] == b' ' as u16
                        && l[i + 1] == b'*' as u16
                        && l[i + 2] == b'*' as u16
                    {
                        push_ascii(&mut out, "## ");
                        out.extend_from_slice(&l[s..i]);
                        push_ascii(&mut out, " ");
                        out.extend_from_slice(&l[i + 3..e - 2]);
                        done = true;
                        break;
                    }
                }
            }
        }
        if !done {
            out.extend_from_slice(&l[s..e]);
        }
        if e == l.len() {
            break;
        }
        out.push(l[e]);
        s = e + 1;
    }
    out
}

const PR_LIT: &[u8] = b"https://github.com/openfrontio/OpenFrontIO/pull/";
const CMP_LIT: &[u8] = b"https://github.com/openfrontio/OpenFrontIO/compare/";

/// Regex 2: bare PR urls -> `[#n](…pull/n)`.
fn replace_pr(l: &[u16]) -> Vec<u16> {
    let mut out: Vec<u16> = Vec::with_capacity(l.len());
    let mut p = 0usize;
    while p < l.len() {
        let q = match find_lit(l, p, PR_LIT) {
            Some(q) => q,
            None => break,
        };
        let d0 = q + PR_LIT.len();
        // lookbehind (?<!\() + leading \b (h is a word unit)
        let ok_anchor = (q == 0 || l[q - 1] != b'(' as u16) && (q == 0 || !is_word_u(l[q - 1]));
        let mut d1 = d0;
        while d1 < l.len() && (0x30..=0x39).contains(&l[d1]) {
            d1 += 1;
        }
        let has_digits = d1 > d0;
        // trailing \b after the greedy digit run: the next unit must be
        // non-word (shrinking the run can never restore the boundary).
        let ok_end = has_digits && (d1 == l.len() || !is_word_u(l[d1]));
        if ok_anchor && ok_end {
            let digits = &l[d0..d1];
            out.extend_from_slice(&l[p..q]);
            push_ascii(&mut out, "[#");
            out.extend_from_slice(digits);
            push_ascii(&mut out, "](https://github.com/openfrontio/OpenFrontIO/pull/");
            out.extend_from_slice(digits);
            out.push(b')' as u16);
            p = d1;
        } else {
            // a failed match advances the scan by ONE unit from the
            // literal start (JS lastIndex semantics), so emit the literal
            // head verbatim and resume inside it.
            out.extend_from_slice(&l[p..=q]);
            p = q + 1;
        }
    }
    out.extend_from_slice(&l[p..]);
    out
}

/// Regex 3: bare compare urls -> `[ref](…compare/ref)`.
fn replace_cmp(l: &[u16]) -> Vec<u16> {
    let mut out: Vec<u16> = Vec::with_capacity(l.len());
    let mut p = 0usize;
    while p < l.len() {
        let q = match find_lit(l, p, CMP_LIT) {
            Some(q) => q,
            None => break,
        };
        let c0 = q + CMP_LIT.len();
        let ok_anchor = (q == 0 || l[q - 1] != b'(' as u16) && (q == 0 || !is_word_u(l[q - 1]));
        // greedy [\w.-]+ run
        let mut run = c0;
        while run < l.len() && (is_word_u(l[run]) || l[run] == b'.' as u16 || l[run] == b'-' as u16) {
            run += 1;
        }
        // shrink until the trailing \b wordness flips across the edge
        let mut e = run;
        while e > c0 {
            let last_word = is_word_u(l[e - 1]);
            let next_word = e < l.len() && is_word_u(l[e]);
            if last_word != next_word {
                break;
            }
            e -= 1;
        }
        if ok_anchor && e > c0 {
            let cap = &l[c0..e];
            out.extend_from_slice(&l[p..q]);
            out.push(b'[' as u16);
            out.extend_from_slice(cap);
            push_ascii(&mut out, "](https://github.com/openfrontio/OpenFrontIO/compare/");
            out.extend_from_slice(cap);
            out.push(b')' as u16);
            p = e;
        } else {
            // one-unit scan advance from the literal start (JS lastIndex).
            out.extend_from_slice(&l[p..=q]);
            p = q + 1;
        }
    }
    out.extend_from_slice(&l[p..]);
    out
}

/// The username group `[a-z\d](?:[a-z\d-]{0,37}[a-z\d])?(?![\w-])` under
/// `i`, greedy-first: the longest valid length (last unit alphanumeric,
/// total <= 39) whose lookahead unit is neither word nor `-`. Returns the
/// length, or 0 when no length passes (the group fails, the engine moves
/// on).
fn uname_len(l: &[u16], at_u: usize) -> usize {
    if at_u >= l.len() || !is_alnum_i(l[at_u]) {
        return 0;
    }
    let mut run = at_u;
    while run < l.len() && is_uname_mid_u(l[run]) {
        run += 1;
    }
    let max_len = (run - at_u).min(39);
    for len in (1..=max_len).rev() {
        if len >= 2 && !is_alnum_i(l[at_u + len - 1]) {
            continue;
        }
        let nxt = at_u + len;
        if nxt == l.len() || (!is_word_u(l[nxt]) && l[nxt] != b'-' as u16) {
            return len;
        }
    }
    0
}

/// Regex 4: @mentions -> `[@name](https://github.com/name)`. At each scan
/// position the prefix alternatives are tried in source order — `^`
/// (zero-width at a line start) then the one-unit `[^\w/[`] class — and a
/// username failure backtracks to the next alternative before the scan
/// advances one unit.
fn replace_men(l: &[u16]) -> Vec<u16> {
    let mut out: Vec<u16> = Vec::with_capacity(l.len());
    let mut p = 0usize;
    while p < l.len() {
        let line_start = p == 0 || is_line_term_u(l[p - 1]);
        let mut done = false;
        // alternative 1: ^ then '@'
        if line_start && l[p] == b'@' as u16 {
            let at_u = p + 1;
            if let Some(len) = nonzero(uname_len(l, at_u)) {
                emit_mention(&mut out, &l[at_u..at_u + len], &[]);
                p = at_u + len;
                done = true;
            }
        }
        // alternative 2: one prefix unit then '@'
        if !done && is_prefix_unit(l[p]) && p + 1 < l.len() && l[p + 1] == b'@' as u16 {
            let at_u = p + 2;
            if let Some(len) = nonzero(uname_len(l, at_u)) {
                emit_mention(&mut out, &l[at_u..at_u + len], &l[p..p + 1]);
                p = at_u + len;
                done = true;
            }
        }
        if !done {
            out.push(l[p]);
            p += 1;
        }
    }
    out
}

fn nonzero(v: usize) -> Option<usize> {
    (v > 0).then_some(v)
}

fn is_prefix_unit(u: u16) -> bool {
    !is_word_u(u) && u != b'/' as u16 && u != b'[' as u16 && u != 0x60
}

fn emit_mention(out: &mut Vec<u16>, name: &[u16], prefix: &[u16]) {
    out.extend_from_slice(prefix);
    push_ascii(out, "[@");
    out.extend_from_slice(name);
    push_ascii(out, "](https://github.com/");
    out.extend_from_slice(name);
    out.push(b')' as u16);
}

/// `String.prototype.replace` chain of NewsMarkdown.ts.
pub fn normalize_news_markdown(markdown: &str) -> String {
    let a = replace_hdr(&units(markdown));
    let b = replace_pr(&a);
    let c = replace_cmp(&b);
    let d = replace_men(&c);
    from_units(&d)
}

/// First occurrence of the ASCII literal `lit` at or after `from`
/// (JS global-regex scanning starts the attempt at every unit, so callers
/// jump straight to the next literal occurrence — the literal is
/// non-self-overlapping, making this equivalent).
fn find_lit(l: &[u16], from: usize, lit: &[u8]) -> Option<usize> {
    if l.len() < lit.len() {
        return None;
    }
    (from..=l.len() - lit.len()).find(|&i| l[i..i + lit.len()].iter().zip(lit).all(|(&u, &b)| u == b as u16))
}

// ---------------------------------------------------------------- vectors op
//
// kind 0: normalizeNewsMarkdown batch — args `[n, (encS input)*n]` ->
//         `[n, (encS result)*n]`.

pub fn run_op(kind: u8, args: &[f64]) -> Vec<f64> {
    let mut i = 0usize;
    match kind {
        0 => {
            let n = args[i] as usize;
            i += 1;
            let mut out = Vec::new();
            out.push(n as f64);
            let inputs: Vec<String> = (0..n).map(|_| read_str(args, &mut i)).collect();
            for md in inputs {
                push_str(&mut out, &normalize_news_markdown(&md));
            }
            out
        }
        k => unreachable!("news_markdown: unknown op kind {k}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hdr_lazy_and_exclusions() {
        assert_eq!(normalize_news_markdown("Title **bold** here"), "Title **bold** here");
        assert_eq!(normalize_news_markdown("- bullet **x**"), "- bullet **x**");
        assert_eq!(normalize_news_markdown("* star **x**"), "* star **x**");
        assert_eq!(normalize_news_markdown("  lead **x**"), "  lead **x**");
        assert_eq!(normalize_news_markdown("A **b**"), "## A b");
        assert_eq!(normalize_news_markdown("A  **b**"), "## A  b");
        assert_eq!(normalize_news_markdown("A **b**c**"), "## A b**c");
        assert_eq!(normalize_news_markdown("A **b** **c**"), "## A b** **c");
        assert_eq!(normalize_news_markdown("A **"), "A **");
        assert_eq!(normalize_news_markdown("A **b**\nB **c**"), "## A b\n## B c");
        assert_eq!(normalize_news_markdown("x\r\nA **b**"), "x\r\n## A b");
    }

    #[test]
    fn pr_url_boundaries() {
        let u = "https://github.com/openfrontio/OpenFrontIO/pull/";
        assert_eq!(
            normalize_news_markdown(&format!("see {u}123 done")),
            "see [#123](https://github.com/openfrontio/OpenFrontIO/pull/123) done"
        );
        assert_eq!(normalize_news_markdown(&format!("({u}123)")), format!("({u}123)"));
        assert_eq!(normalize_news_markdown(&format!("x{u}123")), format!("x{u}123"));
        assert_eq!(normalize_news_markdown(&format!("{u}123abc")), format!("{u}123abc"));
        assert_eq!(
            normalize_news_markdown(&format!("{u}123.4")),
            "[#123](https://github.com/openfrontio/OpenFrontIO/pull/123).4".to_string()
        );
    }

    #[test]
    fn compare_url_trailing_boundary() {
        let u = "https://github.com/openfrontio/OpenFrontIO/compare/";
        assert_eq!(
            normalize_news_markdown(&format!("{u}v1.2.3-rc")),
            "[v1.2.3-rc](https://github.com/openfrontio/OpenFrontIO/compare/v1.2.3-rc)"
        );
        assert_eq!(
            normalize_news_markdown(&format!("{u}abc.")),
            "[abc](https://github.com/openfrontio/OpenFrontIO/compare/abc)."
        );
        assert_eq!(normalize_news_markdown(&format!("{u}-")), format!("{u}-"));
        assert_eq!(normalize_news_markdown(&format!("{u}.")), format!("{u}."));
        assert_eq!(normalize_news_markdown(&format!("({u}x)")), format!("({u}x)"));
    }

    #[test]
    fn mention_semantics() {
        assert_eq!(
            normalize_news_markdown("hi @bob!"),
            "hi [@bob](https://github.com/bob)!"
        );
        assert_eq!(
            normalize_news_markdown("@bob @alice"),
            "[@bob](https://github.com/bob) [@alice](https://github.com/alice)"
        );
        // the consumed prefix blocks the second mention
        assert_eq!(normalize_news_markdown("@a@b"), "[@a](https://github.com/a)@b");
        assert_eq!(normalize_news_markdown("a@b@c"), "a@b@c");
        // i flag keeps original case
        assert_eq!(normalize_news_markdown("@BOB"), "[@BOB](https://github.com/BOB)");
        assert_eq!(normalize_news_markdown("@bob-"), "@bob-");
        assert_eq!(normalize_news_markdown("@bob_x"), "@bob_x");
        assert_eq!(normalize_news_markdown("@us_er"), "@us_er");
        assert_eq!(normalize_news_markdown("@-bob"), "@-bob");
        assert_eq!(normalize_news_markdown("`@bob"), "`@bob");
        assert_eq!(normalize_news_markdown("[@bob"), "[@bob");
        assert_eq!(normalize_news_markdown("/@bob"), "/@bob");
        assert_eq!(
            normalize_news_markdown("line1\n@bob"),
            "line1\n[@bob](https://github.com/bob)"
        );
        assert_eq!(normalize_news_markdown("@1a"), "[@1a](https://github.com/1a)");
        assert_eq!(normalize_news_markdown("@a-b"), "[@a-b](https://github.com/a-b)");
        assert_eq!(normalize_news_markdown("@a--b"), "[@a--b](https://github.com/a--b)");
        // 39 chars match, 40 do not
        let a39 = "@".to_string() + &"a".repeat(39);
        assert!(normalize_news_markdown(&a39).starts_with("[@"));
        let a40 = "@".to_string() + &"a".repeat(40);
        assert_eq!(normalize_news_markdown(&a40), a40);
    }

    #[test]
    fn chain_interaction() {
        let md = "Release **Notes**\nSee https://github.com/openfrontio/OpenFrontIO/pull/42 by @alice and @bob";
        assert_eq!(
            normalize_news_markdown(md),
            "## Release Notes\nSee [#42](https://github.com/openfrontio/OpenFrontIO/pull/42) by [@alice](https://github.com/alice) and [@bob](https://github.com/bob)"
        );
        assert_eq!(
            normalize_news_markdown("@https://github.com/openfrontio/OpenFrontIO/pull/1"),
            "@[#1](https://github.com/openfrontio/OpenFrontIO/pull/1)"
        );
    }

    #[test]
    fn runner_batch() {
        let mut args = vec![1.0];
        push_str(&mut args, "A **b**");
        let r = run_op(0, &args);
        let mut want = vec![1.0];
        push_str(&mut want, "## A b");
        assert_eq!(r, want);
    }
}
