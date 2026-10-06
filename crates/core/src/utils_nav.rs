//! Port of the navigation / server-time / Discord-avatar subset of
//! `src/client/Utils.ts`: `apexPathFor`, `currentPagePath`,
//! `calculateServerTimeOffset`, `getServerNow`,
//! `getSecondsUntilServerTimestamp`, `getDiscordAvatarUrl`.
//!
//! The harness owns the state `currentPagePath` reaches through the imported
//! `pagePin` (a [`PagePinState`] scripted exactly like the ppn_ harness) and
//! the `Date.now()` default-parameter consumption counter the capture scripts
//! through `__UN_NOW` (FIFO, precedent `__CCC_NOW` / `__GCH_RAND`).
//!
//! Faithfulness notes (quirk list):
//!
//! * A JS default parameter fires on `undefined` ONLY — an omitted argument
//!   and an explicit `undefined` both consume the scripted now, an explicit
//!   number never does. `getSecondsUntilServerTimestamp` passes its already
//!   evaluated `localNowMs` EXPLICITLY into `getServerNow`, so the whole
//!   chain consumes `Date.now()` exactly once; the cumulative counter echoed
//!   in every time op's res pins that.
//! * NaN penetrates the seconds clamp: `Math.floor(NaN)` is NaN and
//!   `Math.max(0, NaN)` propagates NaN (js_max), so a NaN target yields NaN,
//!   not the `0` a Rust `f64::max` would return.
//! * `apexPathFor` = `stripVersionPrefix(pathname).path` then the LEADING-only
//!   `/^\/w\d+\//` replace (`/v/c1/w2/w3/x` -> `/w3/x`); the regexes are
//!   anchored and ASCII (`\d` is `[0-9]`, `$` is end-of-input with no
//!   Perl-style trailing-newline leniency).
//! * `currentPagePath` gates on `commit === null` STRICTLY: an empty `path`
//!   still concatenates (`/v/<commit>` bare), and the facade read happens at
//!   most once per scenario (the lazy latch), echoed as the res prefix.
//! * `getDiscordAvatarUrl`: the `user.avatar` gate is TRUTHY (null / "" /
//!   absent all fall through), the id / avatar regexes are the ASCII gates
//!   above, `"a_"` is a UTF-16 byte prefix, and `encodeURIComponent` reuses
//!   [`crate::asset_urls::encode_uri_component`].
//! * The discriminator gate is `!== undefined` STRICT — `null` PASSES
//!   (`Number(null)` is 0 -> `embed/avatars/0.png`), an explicit `undefined`
//!   or an absent field fails to the `null` return.
//! * `Number(user.discriminator) % 5` keeps the JS remainder sign (dividend):
//!   `"-7"` -> `-2` -> `"…/avatars/-2.png"`, and `NaN % 5` / `Infinity % 5`
//!   are NaN, which the template literal spells `"NaN"`. The string coercion
//!   reuses [`crate::game_config_helpers::js_number`] (the `0x10` -> 16,
//!   `"  7  "` -> 7, `"1e1"` -> 10 table).

use crate::asset_urls::encode_uri_component;
use crate::game_config_helpers::js_number;
use crate::js_fixed::js_to_string;
use crate::js_json::{push_val, read_str, read_val, val_field, JsVal};
use crate::jsnum::js_max;
use crate::page_pin::PagePinState;
use crate::server_list::{strip_version_prefix, strip_worker_prefix};

/// JS truthiness over the codec domain.
fn truthy(v: &JsVal) -> bool {
    match v {
        JsVal::Absent | JsVal::Undef | JsVal::Null => false,
        JsVal::Bool(b) => *b,
        JsVal::Num(n) => *n != 0.0 && !n.is_nan(),
        JsVal::Str(s) => !s.is_empty(),
        JsVal::Obj(_) | JsVal::Arr(_) => true,
    }
}

/// `String(v)` for the regex-test coercion of `user.id` (the capture keeps
/// the string domain; the rest of the table keeps the runner total).
fn to_string_of(v: &JsVal) -> String {
    match v {
        JsVal::Str(s) => s.clone(),
        JsVal::Undef | JsVal::Absent => "undefined".to_string(),
        JsVal::Null => "null".to_string(),
        JsVal::Num(n) => js_to_string(*n),
        JsVal::Bool(b) => (if *b { "true" } else { "false" }).to_string(),
        JsVal::Obj(_) | JsVal::Arr(_) => {
            unreachable!("utils_nav: id/avatar containers are out of the capture domain")
        }
    }
}

/// `Number(v)` over the codec domain: `null` is 0, `undefined` NaN, strings
/// ride the [`js_number`] coercion table; containers would need ToPrimitive
/// and never enter the capture.
fn number_of(v: &JsVal) -> f64 {
    match v {
        JsVal::Num(n) => *n,
        JsVal::Bool(b) => if *b { 1.0 } else { 0.0 },
        JsVal::Null => 0.0,
        JsVal::Str(s) => js_number(s),
        JsVal::Undef | JsVal::Absent => f64::NAN,
        JsVal::Obj(_) | JsVal::Arr(_) => f64::NAN, // out of domain, never captured
    }
}

/// `/^\d+$/` — ASCII digits, at least one, end-of-input anchor.
fn is_digits(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

/// `/^[a-f0-9]+$/` — lowercase ASCII hex, at least one.
fn is_hex_lower(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// `getDiscordAvatarUrl(user)`.
fn get_discord_avatar_url(user: &JsVal) -> JsVal {
    if let Some(av) = val_field(user, "avatar") {
        if truthy(av) {
            let id_s = val_field(user, "id")
                .map(to_string_of)
                .unwrap_or_else(|| "undefined".to_string());
            let av_s = to_string_of(av);
            let valid_avatar = is_hex_lower(&av_s) || (av_s.starts_with("a_") && is_hex_lower(&av_s[2..]));
            if is_digits(&id_s) && valid_avatar {
                let ext = if av_s.starts_with("a_") { "gif" } else { "png" };
                return JsVal::Str(format!(
                    "https://cdn.discordapp.com/avatars/{}/{}.{}?size=64",
                    encode_uri_component(&id_s),
                    encode_uri_component(&av_s),
                    ext
                ));
            }
        }
    }
    if let Some(d) = val_field(user, "discriminator") {
        if !matches!(d, JsVal::Undef | JsVal::Absent) {
            let idx = number_of(d) % 5.0;
            return JsVal::Str(format!(
                "https://cdn.discordapp.com/embed/avatars/{}.png",
                js_to_string(idx)
            ));
        }
    }
    JsVal::Null
}

/// The capture harness.
#[derive(Default)]
pub struct RigHarness {
    pin: PagePinState,
    now_consumed: usize,
}

impl RigHarness {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Run one op. Kind table (see `tools/gen_vectors.mjs` `runUN`):
    /// 0 setup `[mode, plen, (path)*plen]` -> `[0]`. mode 0 = the
    ///   `__PPN_PATH` facade returns the path, mode 1 = it THROWS. Resets
    ///   the pin latch, the facade counter and the now consumption counter
    ///   (each time op pushes its own scripted `__UN_NOW` value FIFO-side
    ///   before the call, so the queue never carries across ops).
    /// 1 currentPagePath `[plen, (path)*plen]` -> `[facadeCalls, ...codec(str)]`
    /// 2 calculateServerTimeOffset `[serverTimeMs, mode, nowVal]` -> `[nowConsumed, res]`
    /// 3 getServerNow `[offset, mode, nowVal]` -> `[nowConsumed, res]`
    /// 4 getSecondsUntilServerTimestamp `[target, offset, mode, nowVal]` -> `[nowConsumed, res]`
    ///   (mode 0 = argument omitted, 1 = explicit number, 2 = explicit
    ///   undefined — 0 and 2 consume the scripted now, 1 does not)
    /// 5 apexPathFor `[plen, (path)*plen]` -> `[...codec(str)]`
    /// 6 getDiscordAvatarUrl `[...codec(user)]` -> `[...codec(str|null)]`
    pub fn run_op(&mut self, kind: u8, args: &[f64]) -> Vec<f64> {
        let mut out = Vec::new();
        let mut i = 0usize;
        match kind {
            0 => {
                let mode = args[i] as u8;
                i += 1;
                let path = read_str(args, &mut i);
                self.pin = PagePinState::scripted(if mode == 0 { Some(path) } else { None });
                self.now_consumed = 0;
                vec![0.0]
            }
            1 => {
                let path = read_str(args, &mut i);
                let commit = self.pin.page_pin();
                let res = match commit {
                    Some(c) => JsVal::Str(format!("/v/{c}{path}")),
                    None => JsVal::Str(path),
                };
                out.push(self.pin.facade_calls() as f64);
                push_val(&mut out, &res);
                out
            }
            2 => {
                let server_time = args[i];
                i += 1;
                let mode = args[i] as u8;
                i += 1;
                let now = args[i];
                if mode != 1 {
                    self.now_consumed += 1;
                }
                out.push(self.now_consumed as f64);
                out.push(server_time - now);
                out
            }
            3 => {
                let offset = args[i];
                i += 1;
                let mode = args[i] as u8;
                i += 1;
                let now = args[i];
                if mode != 1 {
                    self.now_consumed += 1;
                }
                out.push(self.now_consumed as f64);
                out.push(now + offset);
                out
            }
            4 => {
                let target = args[i];
                i += 1;
                let offset = args[i];
                i += 1;
                let mode = args[i] as u8;
                i += 1;
                let now = args[i];
                if mode != 1 {
                    self.now_consumed += 1;
                }
                // The outer default parameter is the ONLY Date.now() read:
                // localNowMs is passed explicitly into getServerNow.
                out.push(self.now_consumed as f64);
                out.push(js_max(0.0, ((target - (now + offset)) / 1000.0).floor()));
                out
            }
            5 => {
                let pathname = read_str(args, &mut i);
                let (_, path) = strip_version_prefix(&pathname);
                push_val(&mut out, &JsVal::Str(strip_worker_prefix(&path)));
                out
            }
            6 => {
                let user = read_val(args, &mut i);
                push_val(&mut out, &get_discord_avatar_url(&user));
                out
            }
            k => unreachable!("utils_nav: unknown op kind {k}"),
        }
    }
}
