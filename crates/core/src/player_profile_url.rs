//! Port of `src/client/utilities/PlayerProfileUrl.ts` — the shareable
//! profile-link builder.
//!
//! `playerProfileUrl(publicId)` =
//! `` `${ClientEnv.shareBase()}#modal=profile&publicID=${encodeURIComponent(publicId)}` ``
//!
//! The `ClientEnv.shareBase()` host read crosses as the scripted
//! `__PPU_BASE` facade string (precedent: `__CK_ENV`); `encodeURIComponent`
//! reuses [`crate::asset_urls::encode_uri_component`] (the same JS builtin
//! the asset-URL port already pins).

use crate::asset_urls::encode_uri_component;
use crate::js_json::{push_str, read_str};

/// `playerProfileUrl(publicId)`.
pub fn player_profile_url(share_base: &str, public_id: &str) -> String {
    format!("{share_base}#modal=profile&publicID={}", encode_uri_component(public_id))
}

/// `run_op(kind, args)` — capture harness entry (stateless). Kind table:
/// 0 playerProfileUrl `[blen, (base)*blen, plen, (publicId)*plen]` ->
///   `[len, (charcode)*len]` of the joined URL.
pub fn run_op(kind: u8, args: &[f64]) -> Vec<f64> {
    let mut i = 0usize;
    match kind {
        0 => {
            let base = read_str(args, &mut i);
            let public_id = read_str(args, &mut i);
            let mut out = Vec::new();
            push_str(&mut out, &player_profile_url(&base, &public_id));
            out
        }
        k => unreachable!("player_profile_url: unknown op kind {k}"),
    }
}
