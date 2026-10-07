//! Port of `src/client/hud/HotbarIcons.ts` — the nineteen hotbar icon
//! constants. Each is a LOAD-TIME `assetUrl("images/....svg")` call, i.e.
//! `buildAssetUrl(path, getAssetManifest(), getCdnBase())`. The TS capture
//! scripts `globalThis.__ASSET_MANIFEST__` / `__CDN_BASE__` and re-imports
//! the module with a cache-busting query so the constants re-evaluate per
//! scenario; the Rust twin replays the nineteen-entry path table through
//! [`crate::asset_urls::build_asset_url`] with the scripted manifest / base.
//!
//! Faithfulness notes:
//!
//! * The declaration order below is the export order in the TS file; the
//!   dump walks it verbatim.
//! * `buildAssetUrl` can only throw on a path `normalizeAssetPath` rejects
//!   (a `.` / `..` segment or an empty path) — none of the nineteen literal
//!   paths qualify, so the `[1]` status token is unreachable for this table
//!   but kept for the shared codec shape.
//! * A manifest hit needs a TRUTHY value (an empty-string entry falls
//!   through to the `/encodeAssetPath` branch) and the CDN base is trimmed
//!   of trailing `/` — both live in the reused `asset_urls` implementation,
//!   not duplicated here.

use crate::asset_urls::build_asset_url;
use crate::js_json::{push_str, read_str};

/// The nineteen icon paths in declaration order (the `export const` order of
/// `HotbarIcons.ts`).
pub const ICON_PATHS: [&str; 19] = [
    "images/BattleshipIconWhite.svg",
    "images/CityIconWhite.svg",
    "images/FactoryIconWhite.svg",
    "images/GoldCoinIcon.svg",
    "images/MIRVIcon.svg",
    "images/MissileSiloIconWhite.svg",
    "images/MushroomCloudIconWhite.svg",
    "images/NukeIconWhite.svg",
    "images/PortIcon.svg",
    "images/SamLauncherIconWhite.svg",
    "images/ShieldIconWhite.svg",
    "images/SoldierIcon.svg",
    "images/ClaimIcon.svg",
    "images/ProfileIcon.svg",
    "images/GuildIconWhite.svg",
    "images/TeamIconSolidWhite.svg",
    "images/UpperLimitIcon.svg",
    "images/AllianceIcon.svg",
    "images/TraitorIcon.svg",
];

/// The nineteen export names in the same order (documentation / trace aid).
pub const ICON_NAMES: [&str; 19] = [
    "warshipIcon",
    "cityIcon",
    "factoryIcon",
    "goldCoinIcon",
    "mirvIcon",
    "missileSiloIcon",
    "hydrogenBombIcon",
    "atomBombIcon",
    "portIcon",
    "samLauncherIcon",
    "defensePostIcon",
    "soldierIcon",
    "claimIcon",
    "profileIcon",
    "guildIcon",
    "teamIcon",
    "upperLimitIcon",
    "allianceIcon",
    "traitorIcon",
];

// ---------------------------------------------------------------- vectors op
//
// kind 0: [n, (encS key, encS value)*n, encS cdnBase] ->
//         ([0, encS url] | [1])*19 — the nineteen constants evaluated in
//         declaration order against the scripted manifest / base.

pub fn run_op(kind: u8, args: &[f64]) -> Vec<f64> {
    let mut out = Vec::new();
    let mut i = 0usize;
    match kind {
        0 => {
            let n = args[i] as usize;
            i += 1;
            let manifest: Vec<(String, String)> =
                (0..n).map(|_| (read_str(args, &mut i), read_str(args, &mut i))).collect();
            let base = read_str(args, &mut i);
            for path in ICON_PATHS {
                match build_asset_url(path, &manifest, &base) {
                    Ok(v) => {
                        out.push(0.0);
                        push_str(&mut out, &v);
                    }
                    Err(()) => out.push(1.0),
                }
            }
        }
        k => unreachable!("hotbar_icons: unknown op kind {k}"),
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_shape() {
        assert_eq!(ICON_PATHS.len(), 19);
        assert_eq!(ICON_NAMES.len(), 19);
        // The TS file's first and last declaration.
        assert_eq!(ICON_PATHS[0], "images/BattleshipIconWhite.svg");
        assert_eq!(ICON_PATHS[18], "images/TraitorIcon.svg");
    }

    #[test]
    fn empty_manifest_falls_through_to_encode() {
        let r = run_op(0, &[0.0, 0.0]);
        // 19 successes, each [0, len, units...].
        assert_eq!(r.iter().filter(|&&t| t == 1.0).count(), 0);
        let first_len = r[0];
        assert_eq!(first_len, 0.0); // status token 0
        let url_len = r[1] as usize;
        let units: Vec<u16> = r[2..2 + url_len].iter().map(|&u| u as u16).collect();
        assert_eq!(String::from_utf16_lossy(&units), "/images/BattleshipIconWhite.svg");
    }

    #[test]
    fn manifest_hit_joins_base() {
        // manifest {images/MIRVIcon.svg -> /_assets/mirv.svg}, base "/cdn/".
        let mut a = vec![1.0];
        let k = "images/MIRVIcon.svg";
        a.push(k.encode_utf16().count() as f64);
        a.extend(k.encode_utf16().map(|u| u as f64));
        let v = "/_assets/mirv.svg";
        a.push(v.encode_utf16().count() as f64);
        a.extend(v.encode_utf16().map(|u| u as f64));
        let b = "/cdn";
        a.push(b.encode_utf16().count() as f64);
        a.extend(b.encode_utf16().map(|u| u as f64));
        let r = run_op(0, &a);
        // Walk to the 5th entry (mirvIcon).
        let mut idx = 0usize;
        for e in 0..5 {
            assert_eq!(r[idx], 0.0, "entry {e} status");
            idx += 1;
            let len = r[idx] as usize;
            if e == 4 {
                let units: Vec<u16> =
                    r[idx + 1..idx + 1 + len].iter().map(|&u| u as u16).collect();
                assert_eq!(String::from_utf16_lossy(&units), "/cdn/_assets/mirv.svg");
            }
            idx += 1 + len;
        }
    }
}
