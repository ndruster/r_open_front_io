//! Port of `src/core/AnonNames.ts`.
//!
//! The anonymous-name word bank plus [`anon_word_name`], which maps a join
//! slot (and a per-viewer offset) to a handle: the 125 words fill first
//! (round 0 → bare name), then a round counter suffixes them.
//!
//! Faithfulness notes:
//!
//! * `Math.trunc` / `Math.abs` / `Math.floor` map onto Rust's `trunc` /
//!   `abs` / `floor`, which agree on every input including `NaN`, `±0` and
//!   `±Infinity` (JS `Math.trunc(-0)` is `-0`, `Math.abs(-0)` is `+0`).
//! * `(s + o) % 125` is JS's sign-of-dividend remainder; `s` and `o` are
//!   non-negative after `abs`, and Rust's `f64::rem` matches IEEE `fmod`
//!   here. `NaN % 125` and `±Inf % 125` are `NaN` in both languages.
//! * The word lookup is a JS *property access*: only an integer index in
//!   `[0, 125)` hits the bank (with `-0` hitting index 0, since JS stringifies
//!   the key to `"0"`). `NaN`, `±Infinity`, fractional or out-of-range indices
//!   read `undefined`. When `round === 0` the TS returns that `undefined`
//!   **value** directly (its `: string` annotation is a lie at runtime), so
//!   the port returns [`Option`]; when `round !== 0` the template literal
//!   spells it `"undefined"` inside the handle.
//! * `round === 0` is JS strict equality, so `-0` (from `floor(-0 / 125)`)
//!   takes the bare-name branch.
//! * `${round}` uses JS `Number`→string. `round` is always a non-negative
//!   integer (or `NaN`/`+Infinity` from non-finite slots); JS prints integers
//!   below `1e21` as plain decimal digits, and the parity vectors stay inside
//!   that domain, so [`js_round_str`] formats through `i64` exactly.

/// `ANON_WORDS` — verbatim from the TS source (order is part of the contract:
/// callers index by a deterministic value, so a reordered bank renames players).
pub const ANON_WORDS: [&str; 125] = [
    "Amethyst", "Anchor", "Anvil", "Banner", "Bicycle", "Blizzard", "Bonfire",
    "Bridge", "Bronze", "Cactus", "Castle", "Chariot", "Cipher", "Citadel",
    "Clay", "Cobalt", "Comet", "Compass", "Crimson", "Dusk", "Eclipse", "Ember",
    "Fern", "Fjord", "Flame", "Frost", "Garnet", "Geyser", "Ginger", "Glacier",
    "Goblin", "Gong", "Harbor", "Harp", "Helmet", "Indigo", "Ivory", "Jellyfish",
    "Jungle", "Ladder", "Lagoon", "Lake", "Lantern", "Leather", "Lighthouse",
    "Linen", "Locket", "Lotus", "Magenta", "Mango", "Medallion", "Mermaid",
    "Meteor", "Mirage", "Mist", "Monsoon", "Moss", "Nebula", "Obelisk",
    "Obsidian", "Ocean", "Omen", "Onyx", "Opal", "Oracle", "Pearl", "Plow",
    "Prairie", "Pulley", "Pumpkin", "Pyramid", "Quartz", "Quasar", "Rainbow",
    "Relic", "Riddle", "River", "Ruby", "Sapphire", "Scarlet", "Scepter",
    "Shadow", "Shark", "Shield", "Shovel", "Silence", "Slate", "Sled", "Snail",
    "Sphinx", "Spider", "Steam", "Steel", "Sugar", "Talisman", "Temple",
    "Thistle", "Thunder", "Timber", "Titan", "Topaz", "Torch", "Tornado",
    "Toucan", "Truffle", "Tuba", "Tulip", "Tundra", "Turquoise", "Unicorn",
    "Urchin", "Valley", "Vanilla", "Velvet", "Violin", "Volcano", "Vortex",
    "Waterfall", "Whisper", "Windmill", "Wrench", "Yacht", "Zebra", "Zenith",
    "Zeppelin",
];

/// `ANON_WORDS[idx]` as a JS property access: only an integer in `[0, 125)`
/// resolves (`-0` resolves to index 0); everything else is `undefined`, which
/// the bare-name branch returns as-is and the template literal spells
/// `"undefined"`.
fn word_at(idx: f64) -> Option<&'static str> {
    if idx.is_finite() && (0.0..125.0).contains(&idx) && idx == idx.trunc() {
        Some(ANON_WORDS[idx as usize])
    } else {
        None
    }
}

/// `${round}` — JS `String(Number)` restricted to the domain [`anon_word_name`]
/// can produce: `NaN`, `+Infinity`, or a non-negative exact integer below
/// `1e21` (JS prints those as plain decimal digits; `-0` never reaches here
/// because `round === 0` is checked first and `-0 === 0` is true).
fn js_round_str(v: f64) -> String {
    if v.is_nan() {
        return "NaN".to_string();
    }
    if v == f64::INFINITY {
        return "Infinity".to_string();
    }
    debug_assert!(v == v.trunc() && (0.0..1e21).contains(&v));
    format!("{}", v as i64)
}

/// `anonWordName(slot, offset = 0)`. `offset: None` mirrors the TS default
/// parameter (an omitted argument is `0`). `None` models the JS `undefined`
/// the bare-name branch returns when the word lookup misses.
pub fn anon_word_name(slot: f64, offset: Option<f64>) -> Option<String> {
    let s = slot.trunc().abs();
    let o = offset.unwrap_or(0.0).trunc().abs();
    let animal = word_at((s + o) % 125.0);
    let round = (s / 125.0).floor();
    if round == 0.0 {
        animal.map(str::to_string)
    } else {
        Some(format!(
            "{}{}",
            animal.unwrap_or("undefined"),
            js_round_str(round)
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bare_words_fill_the_first_round() {
        assert_eq!(anon_word_name(0.0, None).as_deref(), Some("Amethyst"));
        assert_eq!(anon_word_name(7.0, Some(0.0)).as_deref(), Some("Bridge"));
        assert_eq!(anon_word_name(124.0, None).as_deref(), Some("Zeppelin"));
    }

    #[test]
    fn round_suffixes_after_the_bank_wraps() {
        assert_eq!(anon_word_name(125.0, None).as_deref(), Some("Amethyst1"));
        assert_eq!(anon_word_name(250.0, None).as_deref(), Some("Amethyst2"));
        assert_eq!(anon_word_name(126.0, None).as_deref(), Some("Anchor1"));
    }

    #[test]
    fn offset_rotates_and_wraps() {
        assert_eq!(anon_word_name(124.0, Some(1.0)).as_deref(), Some("Amethyst"));
        assert_eq!(anon_word_name(10.0, Some(119.0)).as_deref(), Some("Bicycle"));
        assert_eq!(anon_word_name(0.0, Some(-5.0)).as_deref(), Some("Blizzard"));
        assert_eq!(anon_word_name(-3.0, None).as_deref(), Some("Banner"));
    }

    #[test]
    fn fractional_inputs_truncate() {
        assert_eq!(anon_word_name(0.9, None).as_deref(), Some("Amethyst"));
        assert_eq!(anon_word_name(-0.5, None).as_deref(), Some("Amethyst"));
        assert_eq!(anon_word_name(-0.0, None).as_deref(), Some("Amethyst"));
        assert_eq!(anon_word_name(125.7, None).as_deref(), Some("Amethyst1"));
    }

    #[test]
    fn non_finite_inputs_stringify_or_return_undefined() {
        // round != 0: the miss is spelled inside the template literal.
        assert_eq!(
            anon_word_name(f64::NAN, None).as_deref(),
            Some("undefinedNaN")
        );
        assert_eq!(
            anon_word_name(f64::INFINITY, None).as_deref(),
            Some("undefinedInfinity")
        );
        assert_eq!(
            anon_word_name(f64::NEG_INFINITY, None).as_deref(),
            Some("undefinedInfinity")
        );
        // round == 0 with a missed lookup: the TS returns the undefined value.
        assert_eq!(anon_word_name(0.0, Some(f64::NAN)), None);
        assert_eq!(anon_word_name(0.0, Some(f64::INFINITY)), None);
        assert_eq!(anon_word_name(0.0, Some(f64::NEG_INFINITY)), None);
        assert_eq!(
            anon_word_name(f64::NAN, Some(f64::NAN)).as_deref(),
            Some("undefinedNaN")
        );
    }

    #[test]
    fn huge_slots_stay_in_the_exact_integer_range() {
        // 1e18 is divisible by 125, so the index wraps to 0.
        assert_eq!(
            anon_word_name(1e18, None).as_deref(),
            Some("Amethyst8000000000000000")
        );
        // 1e18 + 128 is exactly representable; 128 % 125 = 3.
        assert_eq!(
            anon_word_name(1e18 + 128.0, None).as_deref(),
            Some("Banner8000000000000001")
        );
        // 5 + 1e18 rounds back to 1e18 (ulp(1e18) = 128).
        assert_eq!(anon_word_name(5.0, Some(1e18)).as_deref(), Some("Amethyst"));
    }
}
