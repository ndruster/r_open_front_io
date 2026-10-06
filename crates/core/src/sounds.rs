//! Port of the pure subset of `src/client/sound/Sounds.ts` — the
//! `CUE_CATEGORY` table, the four-key `ambienceUrls` set and `categoryOf`
//! (the `soundEffectUrls` map is an `assetUrl` side effect and the
//! `PlaySoundEffectEvent` / `SetAmbienceEvent` classes ride the `GameEvent`
//! bus — both out of scope).
//!
//! Faithfulness notes (quirk list):
//!
//! * `categoryOf(name)`: `ambienceUrls.has(name) ? "ambience" :
//!   CUE_CATEGORY[name]` — the ambience set wins on the four track keys;
//!   everything else is a plain object read, so an out-of-domain name reads
//!   `undefined` (the harness tags it status 1).
//! * `CUE_CATEGORY` is a plain object literal — `Object.keys` order is the
//!   declaration order (click, click-1..3, slider, nuke-warning, the five
//!   alliance cues + message, then the effects block). `message` maps to
//!   `"alerts"` (it reuses the alliance-suggested asset but the channel is
//!   alerts).
//! * The `ambienceUrls` keys are `"city"`, `"factory"`, `"missile-silo"`,
//!   `"sam-silo"` — a `Map`, so `has` is a key-set test (the asset URLs are
//!   irrelevant to `categoryOf`).

use crate::js_json::{push_str, read_str};

/// The `CUE_CATEGORY` entries in TS declaration order.
pub const CUE_CATEGORY: [(&str, &str); 31] = [
    ("click", "interface"),
    ("click-1", "interface"),
    ("click-2", "interface"),
    ("click-3", "interface"),
    ("slider", "interface"),
    ("nuke-warning", "alerts"),
    ("alliance-suggested", "alerts"),
    ("alliance-accepted", "alerts"),
    ("alliance-declined", "alerts"),
    ("alliance-broken", "alerts"),
    ("message", "alerts"),
    ("atom-launch", "effects"),
    ("atom-hit", "effects"),
    ("hydrogen-launch", "effects"),
    ("hydrogen-hit", "effects"),
    ("mirv-launch", "effects"),
    ("ka-ching", "effects"),
    ("conquered", "effects"),
    ("build-port", "effects"),
    ("build-city", "effects"),
    ("build-defense-post", "effects"),
    ("build-warship", "effects"),
    ("build-factory", "effects"),
    ("build-train-station", "effects"),
    ("sam-built", "effects"),
    ("silo-built", "effects"),
    ("transport-ship", "effects"),
    ("spawn", "effects"),
    ("game-start", "effects"),
    ("victory", "effects"),
    ("defeat", "effects"),
];

/// The `ambienceUrls` key set (the four `AmbienceTrack`s).
pub const AMBIENCE_KEYS: [&str; 4] = ["city", "factory", "missile-silo", "sam-silo"];

/// `categoryOf(name)` — `None` models the out-of-domain `undefined`.
pub fn category_of(name: &str) -> Option<&'static str> {
    if AMBIENCE_KEYS.contains(&name) {
        return Some("ambience");
    }
    CUE_CATEGORY.iter().find(|(k, _)| *k == name).map(|(_, v)| *v)
}

// ---------------------------------------------------------------- vectors op
//
// kind 0: [n, (encS name)*n] -> (0|1, cat?)*n
//         categoryOf batch; 0 = undefined (out of domain), 1 = Some(cat)
//         followed by encS(cat).
// kind 1: [] -> [31, (encS key, encS cat)*31]  the CUE_CATEGORY dump in
//         declaration order.
// kind 2: [] -> [4, (encS key)*4]  the ambience key set.

pub fn run_op(kind: u8, args: &[f64]) -> Vec<f64> {
    let mut out = Vec::new();
    match kind {
        0 => {
            let mut i = 0usize;
            let n = args[i] as usize;
            i += 1;
            for _ in 0..n {
                let name = read_str(args, &mut i);
                match category_of(&name) {
                    Some(c) => {
                        out.push(1.0);
                        push_str(&mut out, c);
                    }
                    None => out.push(0.0),
                }
            }
        }
        1 => {
            out.push(CUE_CATEGORY.len() as f64);
            for (k, v) in CUE_CATEGORY {
                push_str(&mut out, k);
                push_str(&mut out, v);
            }
        }
        2 => {
            out.push(AMBIENCE_KEYS.len() as f64);
            for k in AMBIENCE_KEYS {
                push_str(&mut out, k);
            }
        }
        k => unreachable!("sounds: unknown op kind {k}"),
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_shape() {
        assert_eq!(CUE_CATEGORY.len(), 31);
        assert_eq!(CUE_CATEGORY[0].0, "click");
        assert_eq!(CUE_CATEGORY[30].0, "defeat");
        assert_eq!(CUE_CATEGORY[5].0, "nuke-warning");
        assert_eq!(CUE_CATEGORY[10], ("message", "alerts"));
    }

    #[test]
    fn ambience_wins_and_out_of_domain_reads_undefined() {
        for k in AMBIENCE_KEYS {
            assert_eq!(category_of(k), Some("ambience"));
        }
        assert_eq!(category_of("click"), Some("interface"));
        assert_eq!(category_of("defeat"), Some("effects"));
        assert_eq!(category_of("master"), None);
        assert_eq!(category_of("music"), None);
        assert_eq!(category_of(""), None);
    }

    #[test]
    fn run_op_kind0_tags() {
        // ["city", "nope"] -> [1, encS("ambience"), 0].
        let mut a = vec![2.0];
        push_str(&mut a, "city");
        push_str(&mut a, "nope");
        let r = run_op(0, &a);
        let mut want = vec![1.0];
        push_str(&mut want, "ambience");
        want.push(0.0);
        assert_eq!(r, want);
    }
}
