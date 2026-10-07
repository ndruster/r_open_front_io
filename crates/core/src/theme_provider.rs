//! Port of `src/client/theme/ThemeProvider.ts` — `generateTeamColors` /
//! `buildTeamPalettes` / `SettingsTheme` / the `themeProvider` singleton.
//! The colord surface rides the same capture facade as [`crate::
//! color_allocator`] (see its module docs): every construction is memoized
//! by a deterministic key string, every observation travels in the
//! [`ColordTables`] block, and the Rust twin replays the control flow over
//! the ids. `Math.sin` and `console.warn` are scripted facade tables too
//! (the capture rewrites the call sites through ts_load globals).
//!
//! Faithfulness notes (quirk list, all V8-pinned):
//!
//! * `generateTeamColors`: index 0 is the base instance itself; the 63
//!   variations construct `colord({ l, c, h })` — the object key string is
//!   field-order sensitive (`l` first). `hueShift = (index * 137.508 % 12) -
//!   6`, `h = (lch.h + hueShift + 360) % 360` (JS `%`, dividend sign),
//!   `c = max(10, min(130, lch.c * (1 + 0.1 * sin(index * 0.7)))`, `l =
//!   max(25, min(80, lch.l + 18 * sin(index * 137.508 * PI/180)))` (JS
//!   min/max, NaN-propagating).
//! * `teamColor` re-constructs `colord({ r, g, b })` with `Math.round` of
//!   the base variation's toRgb channels (round-half-up, -0 preserved).
//! * `teamColorForPlayer` caches by playerId BEFORE the modulo; the index is
//!   `simpleHash(playerId) % colors.length`.
//! * `territoryColor`: a non-null team wins over the player type; teamless
//!   BOT with the classic flag off uses the flat Bot team palette, with the
//!   flag on the classic allocator; any other type (incl. out-of-domain)
//!   falls through to the nation allocator.
//! * `structureColors`: the LAB pair mutates `l` in place; each `colord`
//!   construction of the mutated object is a NEW memoized id (the key
//!   string changes), so the loop walks fresh delta rows. The runaway warn
//!   fires when `loopCount > 50` (51 processed iterations) and its text
//!   embeds `toRgbString()` of the LAB-object colors and `String(contrast)`.
//! * `borderColor`: the `{...hsl, l: hsl.l * scale}` spread keeps the
//!   `h,s,l,a` key order (the override does NOT move `l` to the end).
//! * `themeProvider.current()`: `overrides.palette ?? "default"` (nullish →
//!   default, a non-string key misses THEMES), then the `useClassicBotColors`
//!   write — on an undefined theme that is the TypeError "Cannot set
//!   properties of undefined (setting 'useClassicBotColors')"; an undefined
//!   overrides object throws the read TypeError "Cannot read properties of
//!   undefined (reading 'palette')". `reset()` recreates both themes (the
//!   only observable is the step marker).

use crate::color_allocator::{key_hex, key_obj, ColorAllocator, ColordTables};
use crate::js_fixed::js_to_string;
use crate::js_json::{push_str, read_str, read_val, val_field, JsVal};
use crate::jsnum::{js_max, js_min, js_mod, js_round};
use crate::render_settings::create_theme_settings;
use crate::util::simple_hash;

/// `generateTeamColors`: the base id plus 63 golden-angle variations.
pub fn generate_team_colors(base: u32, t: &ColordTables) -> Vec<u32> {
    let lch = t.lch(base);
    let golden_angle = 137.508f64;
    let mut out = vec![base];
    for index in 1..64 {
        let hue_shift = js_mod(index as f64 * golden_angle, 12.0) - 6.0;
        let h = js_mod(lch[2] + hue_shift + 360.0, 360.0);
        let chroma_factor = 1.0 + 0.1 * t.sin(index as f64 * 0.7);
        let c = js_max(10.0, js_min(130.0, lch[1] * chroma_factor));
        let light_offset =
            18.0 * t.sin(index as f64 * golden_angle * (std::f64::consts::PI / 180.0));
        let l = js_max(25.0, js_min(80.0, lch[0] + light_offset));
        out.push(t.id_of(&key_obj(&[("l", l), ("c", c), ("h", h)])));
    }
    out
}

/// `buildTeamPalettes`: the Bot team stays a single flat color.
pub fn build_team_palettes(team_colors: &[(String, String)], t: &ColordTables) -> Vec<(String, Vec<u32>)> {
    team_colors
        .iter()
        .map(|(team, hex)| {
            let base = t.id_of(&key_hex(hex));
            let palette = if team == "Bot" {
                vec![base]
            } else {
                generate_team_colors(base, t)
            };
            (team.clone(), palette)
        })
        .collect()
}

/// `SettingsTheme` over the facade tables.
pub struct SettingsTheme<'t> {
    human: ColorAllocator,
    nation: ColorAllocator,
    classic_bot: ColorAllocator,
    palettes: Vec<(String, Vec<u32>)>,
    team_player_colors: Vec<(String, u32)>,
    use_classic_bot_colors: bool,
    focused: u32,
    spawn: u32,
    border_darken: f64,
    border_lightness_scale: f64,
    defended_light: f64,
    defended_dark: f64,
    contrast_target: f64,
    t: &'t ColordTables,
}

fn hex_list_field(settings: &JsVal, key: &str) -> Vec<String> {
    match val_field(settings, key) {
        Some(JsVal::Arr(items)) => items
            .iter()
            .map(|it| match it {
                JsVal::Str(s) => s.clone(),
                _ => panic!("theme_provider: non-string palette entry"),
            })
            .collect(),
        _ => panic!("theme_provider: missing palette field {key}"),
    }
}

fn num_field(settings: &JsVal, key: &str) -> f64 {
    match val_field(settings, key) {
        Some(JsVal::Num(n)) => *n,
        _ => panic!("theme_provider: missing knob field {key}"),
    }
}

fn str_field(settings: &JsVal, key: &str) -> String {
    match val_field(settings, key) {
        Some(JsVal::Str(s)) => s.clone(),
        _ => panic!("theme_provider: missing color field {key}"),
    }
}

impl<'t> SettingsTheme<'t> {
    /// `new SettingsTheme(settings)`.
    pub fn new(settings: &JsVal, t: &'t ColordTables) -> Self {
        let human_ids: Vec<u32> = hex_list_field(settings, "humanColors")
            .iter()
            .map(|h| t.id_of(&key_hex(h)))
            .collect();
        let nation_ids: Vec<u32> = hex_list_field(settings, "nationColors")
            .iter()
            .map(|h| t.id_of(&key_hex(h)))
            .collect();
        let classic_ids: Vec<u32> = hex_list_field(settings, "classicBotColors")
            .iter()
            .map(|h| t.id_of(&key_hex(h)))
            .collect();
        let fallback_ids: Vec<u32> = hex_list_field(settings, "fallbackColors")
            .iter()
            .map(|h| t.id_of(&key_hex(h)))
            .collect();
        let team_colors: Vec<(String, String)> = match val_field(settings, "teamColors") {
            Some(JsVal::Obj(fields)) => fields
                .iter()
                .map(|(k, v)| {
                    let JsVal::Str(s) = v else {
                        panic!("theme_provider: non-string team color")
                    };
                    (k.clone(), s.clone())
                })
                .collect(),
            _ => panic!("theme_provider: missing teamColors"),
        };
        Self {
            human: ColorAllocator::new(&human_ids, &fallback_ids),
            nation: ColorAllocator::new(&nation_ids, &nation_ids),
            classic_bot: ColorAllocator::new(&classic_ids, &classic_ids),
            palettes: build_team_palettes(&team_colors, t),
            team_player_colors: Vec::new(),
            use_classic_bot_colors: false,
            focused: t.id_of(&key_hex(&str_field(settings, "focusedBorderColor"))),
            spawn: t.id_of(&key_hex(&str_field(settings, "spawnHighlightColor"))),
            border_darken: num_field(settings, "borderDarken"),
            border_lightness_scale: num_field(settings, "borderLightnessScale"),
            defended_light: num_field(settings, "defendedBorderDarkenLight"),
            defended_dark: num_field(settings, "defendedBorderDarkenDark"),
            contrast_target: num_field(settings, "structureContrastTarget"),
            t,
        }
    }

    /// `teamColorVariations` (private): the palette, else the human
    /// allocator's assignment of the team name.
    fn team_color_variations(&mut self, team: &str) -> Vec<u32> {
        if let Some((_, p)) = self.palettes.iter().find(|(k, _)| k == team) {
            return p.clone();
        }
        vec![self.human.assign_color(team, self.t)]
    }

    /// `teamColor`: the first variation, re-quantized through
    /// `colord({ r: round, g: round, b: round })`.
    pub fn team_color(&mut self, team: &str) -> u32 {
        let rgb = self.t.rgb(self.team_color_variations(team)[0]);
        self.t.id_of(&key_obj(&[
            ("r", js_round(rgb[0])),
            ("g", js_round(rgb[1])),
            ("b", js_round(rgb[2])),
        ]))
    }

    /// `teamColorForPlayer`: the playerId cache, else the hashed variation.
    pub fn team_color_for_player(&mut self, team: &str, player_id: &str) -> u32 {
        if let Some((_, c)) = self.team_player_colors.iter().find(|(k, _)| k == player_id) {
            return *c;
        }
        let colors = self.team_color_variations(team);
        let color = colors[simple_hash(player_id) as usize % colors.len()];
        self.team_player_colors
            .push((player_id.to_string(), color));
        color
    }

    /// `territoryColor` over the PlayerView codec triple.
    pub fn territory_color(
        &mut self,
        team: Option<&str>,
        player_id: &str,
        player_type: &str,
    ) -> u32 {
        if let Some(team) = team {
            return self.team_color_for_player(team, player_id);
        }
        if player_type == "HUMAN" {
            return self.human.assign_color(player_id, self.t);
        }
        if player_type == "BOT" {
            if self.use_classic_bot_colors {
                return self.classic_bot.assign_color(player_id, self.t);
            }
            return self.team_color_for_player("Bot", player_id);
        }
        self.nation.assign_color(player_id, self.t)
    }

    /// `borderColor`: the HSL lightness scale then the absolute darken.
    pub fn border_color(&self, territory: u32) -> u32 {
        let mut out = territory;
        if self.border_lightness_scale != 1.0 {
            let hsl = self.t.hsl(out);
            out = self.t.id_of(&key_obj(&[
                ("h", hsl[0]),
                ("s", hsl[1]),
                ("l", hsl[2] * self.border_lightness_scale),
                ("a", hsl[3]),
            ]));
        }
        if self.border_darken != 0.0 {
            out = self.t.darken(out, self.border_darken);
        }
        out
    }

    /// `defendedBorderColors`.
    pub fn defended_border_colors(&self, territory: u32) -> (u32, u32) {
        (
            self.t.darken(territory, self.defended_light),
            self.t.darken(territory, self.defended_dark),
        )
    }

    /// `contrast` (private): the delta row of the two LAB-object colors.
    fn contrast(&self, light_lab: [f64; 4], dark_lab: [f64; 4]) -> f64 {
        let l = self.t.id_of(&key_obj(&[
            ("l", light_lab[0]),
            ("a", light_lab[1]),
            ("b", light_lab[2]),
            ("alpha", light_lab[3]),
        ]));
        let d = self.t.id_of(&key_obj(&[
            ("l", dark_lab[0]),
            ("a", dark_lab[1]),
            ("b", dark_lab[2]),
            ("alpha", dark_lab[3]),
        ]));
        self.t.delta(l, d)
    }

    /// `structureColors`: returns (light id, dark id, warn text option).
    pub fn structure_colors(&self, territory: u32) -> (u32, u32, Option<String>) {
        let mut light = self.t.lab(self.t.alpha(territory, 150.0 / 255.0));
        let mut dark = self.t.lab(self.border_color(territory));
        let mut contrast = self.contrast(light, dark);
        let mut loop_count = 0.0f64;
        let warn = loop {
            // `while (contrast < contrastTarget)`: a NaN contrast never
            // enters the loop body (the negated strict-less-than test).
            #[allow(clippy::neg_cmp_op_on_partial_ord)]
            if !(contrast < self.contrast_target) {
                break None;
            }
            if loop_count > 50.0 {
                let light_rgb = self
                    .t
                    .id_of(&key_obj(&[
                        ("l", light[0]),
                        ("a", light[1]),
                        ("b", light[2]),
                        ("alpha", light[3]),
                    ]));
                let dark_rgb = self
                    .t
                    .id_of(&key_obj(&[
                        ("l", dark[0]),
                        ("a", dark[1]),
                        ("b", dark[2]),
                        ("alpha", dark[3]),
                    ]));
                break Some(format!(
                    "Infinite loop detected during structure color calculation.\n          Light color: {},\n          Dark color: {},\n          Contrast: {}",
                    self.t.rgb_string(light_rgb),
                    self.t.rgb_string(dark_rgb),
                    js_to_string(contrast),
                ));
            } else if loop_count > 10.0 {
                light[0] = clamp(light[0] + 5.0);
            } else {
                dark[0] = clamp(dark[0] - 5.0);
            }
            contrast = self.contrast(light, dark);
            loop_count += 1.0;
        };
        let light_id = self.t.id_of(&key_obj(&[
            ("l", light[0]),
            ("a", light[1]),
            ("b", light[2]),
            ("alpha", light[3]),
        ]));
        let dark_id = self.t.id_of(&key_obj(&[
            ("l", dark[0]),
            ("a", dark[1]),
            ("b", dark[2]),
            ("alpha", dark[3]),
        ]));
        (light_id, dark_id, warn)
    }

    /// `focusedBorderColor` / `spawnHighlightColor`.
    pub fn focused_border_color(&self) -> u32 {
        self.focused
    }

    pub fn spawn_highlight_color(&self) -> u32 {
        self.spawn
    }
}

/// `Theme.clamp` (private): `Math.min(Math.max(0, num), 100)`.
fn clamp(num: f64) -> f64 {
    js_min(js_max(0.0, num), 100.0)
}

/// `themeProvider.current()` over the scripted overrides: `Ok((theme index
/// 0=default/1=colorblind, useClassicBotColors flag))` or the TypeError
/// message text.
pub fn provider_current(overrides: &JsVal) -> Result<(usize, bool), String> {
    let palette = match overrides {
        JsVal::Undef | JsVal::Absent => {
            return Err("Cannot read properties of undefined (reading 'palette')".to_string())
        }
        JsVal::Null => {
            return Err("Cannot read properties of null (reading 'palette')".to_string())
        }
        JsVal::Obj(_) => match val_field(overrides, "palette") {
            Some(JsVal::Str(s)) => Some(s.clone()),
            None | Some(JsVal::Undef) | Some(JsVal::Null) | Some(JsVal::Absent) => None,
            // A non-string key coerces to a property name no theme has.
            Some(other) => Some(js_to_string_val(other)),
        },
        // A boxed primitive / exotic: `.palette` reads undefined.
        _ => None,
    };
    let idx = match palette.as_deref().unwrap_or("default") {
        "default" => 0,
        "colorblind" => 1,
        _ => {
            return Err(
                "Cannot set properties of undefined (setting 'useClassicBotColors')".to_string(),
            )
        }
    };
    let flag = match val_field(overrides, "classicBotColors") {
        Some(JsVal::Bool(b)) => *b,
        // `?? false` only intercepts nullish; any other present value keeps
        // its truthiness for both the write and the `? 1 : 0` dump.
        Some(JsVal::Num(n)) => *n != 0.0 && !n.is_nan(),
        Some(JsVal::Str(s)) => !s.is_empty(),
        Some(JsVal::Obj(_) | JsVal::Arr(_)) => true,
        _ => false,
    };
    Ok((idx, flag))
}

/// The JS string coercion of a non-string override key (numbers / booleans
/// go through Number::toString; objects / arrays never match a theme name
/// and ride their stringify text).
fn js_to_string_val(v: &JsVal) -> String {
    match v {
        JsVal::Num(n) => js_to_string(*n),
        JsVal::Bool(b) => if *b { "true" } else { "false" }.to_string(),
        _ => "x".to_string(),
    }
}

// ---------------------------------------------------------------- vectors op
//
// kind 0: buildTeamPalettes — args `[...codec name, (colord tables)]` ->
//         `[size, (n, (r, g, b, a)*n)*size]` (palette order = teamColors
//         entry order).
// kind 1: SettingsTheme sequence — args `[...codec name, flag 1|0, (tables),
//         n, (step)*n]` -> `[n, (0, r, g, b, a)*n]`. Step codec:
//         `[0, encS team]` teamColor, `[1, encS team, encS playerId]`
//         teamColorForPlayer, `[2, (0|null | 1, encS team), encS id,
//         encS type]` territoryColor, `[3, encS hex]` borderColor,
//         `[4, encS hex]` defendedBorderColors (8 floats), `[5]` focused,
//         `[6]` spawn.
// kind 2: structureColors — args `[encS hex, target, scale, darken,
//         (tables)]` -> `[0, (r,g,b,a)*2, 0] | [0, (r,g,b,a)*2, 1, encS
//         warn text]`.
// kind 3: themeProvider steps — args `[n, ([0, ...codec overrides] |
//         [1 reset])*n]` -> `[n, ([0, themeIdx, flag] | [1, encS TypeError
//         msg] | [2])*n]`.

/// Flat-token runner shared by the golden replay and the wasm probe.
pub fn run_op(kind: u8, args: &[f64]) -> Vec<f64> {
    let mut i = 0usize;
    match kind {
        0 => {
            let name = read_val(args, &mut i);
            let t = ColordTables::read(args, &mut i);
            let settings = create_theme_settings(Some(&name))
                .expect("theme_provider: scripted theme name must exist");
            let team_colors: Vec<(String, String)> = match val_field(&settings, "teamColors") {
                Some(JsVal::Obj(fields)) => fields
                    .iter()
                    .map(|(k, v)| {
                        let JsVal::Str(s) = v else {
                            panic!("theme_provider: non-string team color")
                        };
                        (k.clone(), s.clone())
                    })
                    .collect(),
                _ => panic!("theme_provider: missing teamColors"),
            };
            let palettes = build_team_palettes(&team_colors, &t);
            let mut out = vec![palettes.len() as f64];
            for (_, cols) in &palettes {
                out.push(cols.len() as f64);
                for c in cols {
                    out.extend_from_slice(&t.rgb(*c));
                }
            }
            out
        }
        1 => {
            let name = read_val(args, &mut i);
            let flag = args[i] != 0.0;
            i += 1;
            let t = ColordTables::read(args, &mut i);
            let settings = create_theme_settings(Some(&name))
                .expect("theme_provider: scripted theme name must exist");
            let mut theme = SettingsTheme::new(&settings, &t);
            theme.use_classic_bot_colors = flag;
            let n = args[i] as usize;
            i += 1;
            let mut out = vec![n as f64];
            for _ in 0..n {
                let step = args[i] as u8;
                i += 1;
                let id = match step {
                    0 => {
                        let team = read_str(args, &mut i);
                        theme.team_color(&team)
                    }
                    1 => {
                        let team = read_str(args, &mut i);
                        let pid = read_str(args, &mut i);
                        theme.team_color_for_player(&team, &pid)
                    }
                    2 => {
                        let team = if args[i] == 0.0 {
                            i += 1;
                            None
                        } else {
                            i += 1;
                            Some(read_str(args, &mut i))
                        };
                        let pid = read_str(args, &mut i);
                        let ptype = read_str(args, &mut i);
                        theme.territory_color(team.as_deref(), &pid, &ptype)
                    }
                    3 => {
                        let hex = read_str(args, &mut i);
                        theme.border_color(t.id_of(&key_hex(&hex)))
                    }
                    4 => {
                        let hex = read_str(args, &mut i);
                        let (l, d) = theme.defended_border_colors(t.id_of(&key_hex(&hex)));
                        out.push(0.0);
                        out.extend_from_slice(&t.rgb(l));
                        out.extend_from_slice(&t.rgb(d));
                        continue;
                    }
                    5 => theme.focused_border_color(),
                    _ => theme.spawn_highlight_color(),
                };
                out.push(0.0);
                out.extend_from_slice(&t.rgb(id));
            }
            out
        }
        2 => {
            let hex = read_str(args, &mut i);
            let target = args[i];
            let scale = args[i + 1];
            let darken = args[i + 2];
            i += 3;
            let t = ColordTables::read(args, &mut i);
            let mut settings = create_theme_settings(Some(&JsVal::Str("default".to_string())))
                .expect("theme_provider: default theme exists");
            let JsVal::Obj(fields) = &mut settings else {
                unreachable!("theme_provider: settings object")
            };
            crate::js_json::map_set(fields, "borderLightnessScale", JsVal::Num(scale));
            crate::js_json::map_set(fields, "borderDarken", JsVal::Num(darken));
            crate::js_json::map_set(fields, "structureContrastTarget", JsVal::Num(target));
            let theme = SettingsTheme::new(&settings, &t);
            let (l, d, warn) = theme.structure_colors(t.id_of(&key_hex(&hex)));
            let mut out = vec![0.0];
            out.extend_from_slice(&t.rgb(l));
            out.extend_from_slice(&t.rgb(d));
            match warn {
                None => out.push(0.0),
                Some(text) => {
                    out.push(1.0);
                    push_str(&mut out, &text);
                }
            }
            out
        }
        3 => {
            let n = args[i] as usize;
            i += 1;
            let mut out = vec![n as f64];
            for _ in 0..n {
                let step = args[i] as u8;
                i += 1;
                if step == 1 {
                    out.push(2.0);
                    continue;
                }
                let overrides = read_val(args, &mut i);
                match provider_current(&overrides) {
                    Ok((idx, flag)) => {
                        out.push(0.0);
                        out.push(idx as f64);
                        out.push(f64::from(flag));
                    }
                    Err(msg) => {
                        out.push(1.0);
                        push_str(&mut out, &msg);
                    }
                }
            }
            out
        }
        k => unreachable!("theme_provider: unknown op kind {k}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::js_json::push_val;

    fn s(v: &str) -> JsVal {
        JsVal::Str(v.to_string())
    }
    fn arr(items: &[&str]) -> JsVal {
        JsVal::Arr(items.iter().map(|x| s(x)).collect())
    }

    /// A minimal `ThemeSettings` stand-in: only the Bot team (the flat
    /// palette, no `generateTeamColors` machinery), single-entry nation /
    /// classic pools, two-entry human pool. `target` is the scripted
    /// `structureContrastTarget`.
    fn small_settings(target: f64) -> JsVal {
        JsVal::Obj(vec![
            (
                "teamColors".to_string(),
                JsVal::Obj(vec![("Bot".to_string(), s("#b0"))]),
            ),
            ("humanColors".to_string(), arr(&["#h1", "#h2"])),
            ("nationColors".to_string(), arr(&["#n1"])),
            ("classicBotColors".to_string(), arr(&["#c1"])),
            ("fallbackColors".to_string(), arr(&["#f1"])),
            ("borderDarken".to_string(), JsVal::Num(0.125)),
            ("borderLightnessScale".to_string(), JsVal::Num(1.0)),
            ("defendedBorderDarkenLight".to_string(), JsVal::Num(0.2)),
            ("defendedBorderDarkenDark".to_string(), JsVal::Num(0.4)),
            ("structureContrastTarget".to_string(), JsVal::Num(target)),
            ("focusedBorderColor".to_string(), s("#e1")),
            ("spawnHighlightColor".to_string(), s("#s1")),
        ])
    }

    /// The dispatch fixture: hexes -> ids 1..9, rgb fingerprints `[id,0,0,1]`,
    /// the Bot teamColor re-quantize (id 10), the Zebra human-pool requantize
    /// ids (11/12), border/defended darkens (20/21/22) and the two
    /// structureColors chains: territory #b0 -> (31,32) with delta 0.2 and
    /// territory #r0 -> (42,43) with a NaN delta.
    fn disp_fixture() -> ColordTables {
        let mut t = ColordTables::default();
        let hexes = ["#b0", "#r0", "#h1", "#h2", "#n1", "#c1", "#f1", "#e1", "#s1"];
        for (k, h) in hexes.iter().enumerate() {
            let id = (k + 1) as u32;
            t.construct.push((key_hex(h), id));
            t.rgb.push((id, [f64::from(id), 0.0, 0.0, 1.0]));
        }
        let objs: &[(&[(&str, f64)], u32)] = &[
            (&[("r", 1.0), ("g", 0.0), ("b", 0.0)], 10),
            (&[("r", 3.0), ("g", 0.0), ("b", 0.0)], 11),
            (&[("r", 4.0), ("g", 0.0), ("b", 0.0)], 12),
            (
                &[("l", 100.0), ("a", 0.0), ("b", 0.0), ("alpha", 1.0)],
                31,
            ),
            (&[("l", 0.0), ("a", 0.0), ("b", 0.0), ("alpha", 1.0)], 32),
            (&[("l", 50.0), ("a", 0.0), ("b", 0.0), ("alpha", 1.0)], 42),
            (&[("l", 40.0), ("a", 0.0), ("b", 0.0), ("alpha", 1.0)], 43),
        ];
        for (fields, id) in objs {
            t.construct.push((key_obj(fields), *id));
        }
        t.darken
            .extend_from_slice(&[(1, 0.125, 20), (1, 0.2, 21), (1, 0.4, 22), (2, 0.125, 41)]);
        t.alpha.push((1, 150.0 / 255.0, 30));
        t.alpha.push((2, 150.0 / 255.0, 40));
        t.lab.push((30, [100.0, 0.0, 0.0, 1.0]));
        t.lab.push((20, [0.0, 0.0, 0.0, 1.0]));
        t.lab.push((40, [50.0, 0.0, 0.0, 1.0]));
        t.lab.push((41, [40.0, 0.0, 0.0, 1.0]));
        t.delta.push((31, 32, 0.2));
        t.delta.push((42, 43, f64::NAN));
        for (a, b) in [(3u32, 3u32), (4, 4), (5, 5), (7, 7)] {
            t.delta.push((a, b, 0.0));
        }
        for (a, b) in [(3u32, 4u32), (4, 3), (3, 7), (4, 7), (7, 3), (7, 4)] {
            t.delta.push((a, b, 0.5));
        }
        t.rgb_string
            .push((31, "rgb(255, 0, 0)".to_string()));
        t.rgb_string
            .push((32, "rgb(0, 0, 0)".to_string()));
        t
    }

    #[test]
    fn key_obj_stringifies_like_js_string() {
        // -0 prints "0", NaN prints "NaN"; field order is the literal order.
        assert_eq!(
            key_obj(&[("l", 1.5), ("c", -0.0), ("h", f64::NAN)]),
            "Ol=1.5;c=0;h=NaN;"
        );
        assert_eq!(key_obj(&[]), "O;");
    }

    #[test]
    fn dispatch_team_wins_and_pools_route() {
        let t = disp_fixture();
        let settings = small_settings(0.1);
        let mut theme = SettingsTheme::new(&settings, &t);
        // A team wins over the player type.
        assert_eq!(theme.territory_color(Some("Bot"), "p1", "HUMAN"), 1);
        // teamColorForPlayer caches before the modulo.
        assert_eq!(theme.team_color_for_player("Bot", "u1"), 1);
        assert_eq!(theme.team_color_for_player("Bot", "u1"), 1);
        // teamColor re-quantizes through colord({r,g,b}).
        assert_eq!(theme.team_color("Bot"), 10);
        // Unknown team falls through the human allocator (random first pick).
        assert!(matches!(theme.team_color("Zebra"), 11 | 12));
        // HUMAN -> human pool {3,4} (fallback #f1=7 only after a drain).
        let h = theme.territory_color(None, "h1", "HUMAN");
        assert!(matches!(h, 3 | 4));
        let h2 = theme.territory_color(None, "h2", "HUMAN");
        assert!(matches!(h2, 3 | 4 | 7));
        assert_ne!(h, h2);
        // BOT flag off -> flat Bot team color; flag on -> classic pool.
        assert_eq!(theme.territory_color(None, "b1", "BOT"), 1);
        theme.use_classic_bot_colors = true;
        assert_eq!(theme.territory_color(None, "b1", "BOT"), 6);
        // NATION and out-of-domain types share the nation pool.
        assert_eq!(theme.territory_color(None, "n1", "NATION"), 5);
        assert_eq!(theme.territory_color(None, "w1", "WEIRD"), 5);
        // Border / defended / fixed colors.
        assert_eq!(theme.border_color(1), 20);
        assert_eq!(theme.defended_border_colors(1), (21, 22));
        assert_eq!(theme.focused_border_color(), 8);
        assert_eq!(theme.spawn_highlight_color(), 9);
    }

    #[test]
    fn structure_loop_none_and_nan_shortcircuit() {
        let t = disp_fixture();
        let settings = small_settings(0.1);
        let theme = SettingsTheme::new(&settings, &t);
        // contrast 0.2 >= target 0.1 -> no loop.
        assert_eq!(theme.structure_colors(1), (31, 32, None));
        // A NaN contrast never satisfies `contrast < target`.
        assert_eq!(theme.structure_colors(2), (42, 43, None));
    }

    #[test]
    fn structure_warn_after_51_loops() {
        let t = disp_fixture();
        let settings = small_settings(f64::INFINITY);
        let theme = SettingsTheme::new(&settings, &t);
        // The clamp endpoints pin both l values, so the loop re-walks the
        // same delta row until loopCount > 50 fires the warn.
        let (light, dark, warn) = theme.structure_colors(1);
        assert_eq!((light, dark), (31, 32));
        assert_eq!(
            warn.unwrap(),
            "Infinite loop detected during structure color calculation.\n          \
             Light color: rgb(255, 0, 0),\n          Dark color: rgb(0, 0, 0),\n          \
             Contrast: 0.2"
        );
    }

    #[test]
    fn provider_current_gate_and_typeerrors() {
        let obj = |fields: Vec<(&str, JsVal)>| {
            JsVal::Obj(fields.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
        };
        assert_eq!(
            provider_current(&JsVal::Undef),
            Err("Cannot read properties of undefined (reading 'palette')".to_string())
        );
        assert_eq!(
            provider_current(&JsVal::Null),
            Err("Cannot read properties of null (reading 'palette')".to_string())
        );
        assert_eq!(provider_current(&obj(vec![])), Ok((0, false)));
        assert_eq!(
            provider_current(&obj(vec![
                ("palette", s("colorblind")),
                ("classicBotColors", JsVal::Bool(true)),
            ])),
            Ok((1, true))
        );
        // `?? "default"` only intercepts nullish.
        assert_eq!(
            provider_current(&obj(vec![("palette", JsVal::Null)])),
            Ok((0, false))
        );
        // A miss on THEMES makes the write throw.
        assert_eq!(
            provider_current(&obj(vec![("palette", s("bogus"))])),
            Err(
                "Cannot set properties of undefined (setting 'useClassicBotColors')"
                    .to_string()
            )
        );
        // A non-string key stringifies; 5 -> "5" misses.
        assert_eq!(
            provider_current(&obj(vec![("palette", JsVal::Num(5.0))])),
            Err(
                "Cannot set properties of undefined (setting 'useClassicBotColors')"
                    .to_string()
            )
        );
        // `?? false` keeps truthiness of any present value.
        let flag = |v: JsVal| provider_current(&obj(vec![("classicBotColors", v)])).unwrap().1;
        assert!(!flag(JsVal::Num(0.0)));
        assert!(!flag(JsVal::Num(f64::NAN)));
        assert!(flag(JsVal::Num(2.0)));
        assert!(!flag(JsVal::Str(String::new())));
        assert!(flag(JsVal::Str("x".to_string())));
        // A primitive box reads .palette undefined -> default theme.
        assert_eq!(provider_current(&JsVal::Num(3.0)), Ok((0, false)));
    }

    #[test]
    fn run_op_kind3_steps() {
        let mut args = vec![4.0];
        args.push(0.0);
        push_val(&mut args, &JsVal::Obj(Vec::new()));
        args.push(1.0); // reset marker
        args.push(0.0);
        push_val(&mut args, &JsVal::Undef);
        args.push(0.0);
        push_val(
            &mut args,
            &JsVal::Obj(vec![("palette".to_string(), JsVal::Str("bogus".to_string()))]),
        );
        let got = run_op(3, &args);
        let mut i = 0usize;
        assert_eq!(got[i], 4.0);
        i += 1;
        assert_eq!(got[i], 0.0);
        assert_eq!((got[i + 1], got[i + 2]), (0.0, 0.0));
        i += 3;
        assert_eq!(got[i], 2.0); // reset marker
        i += 1;
        assert_eq!(got[i], 1.0);
        i += 1;
        assert_eq!(
            read_str(&got, &mut i),
            "Cannot read properties of undefined (reading 'palette')"
        );
        assert_eq!(got[i], 1.0);
        i += 1;
        assert_eq!(
            read_str(&got, &mut i),
            "Cannot set properties of undefined (setting 'useClassicBotColors')"
        );
        assert_eq!(i, got.len());
    }

    #[test]
    fn generate_team_colors_sixty_four_variations() {
        // sin = 0 pins c/l at the base (clamped), h sweeps the golden-angle
        // band; every index gets its own memoized object id.
        let mut t = ColordTables::default();
        t.construct.push((key_hex("#b0"), 1));
        t.construct.push((key_hex("#r0"), 2));
        t.lch.push((2, [50.0, 80.0, 12.0, 1.0]));
        let golden_angle = 137.508f64;
        for index in 1..64u32 {
            t.sin.push((f64::from(index) * 0.7, 0.0));
            t.sin.push((
                f64::from(index) * golden_angle * (std::f64::consts::PI / 180.0),
                0.0,
            ));
            let hue_shift = js_mod(f64::from(index) * golden_angle, 12.0) - 6.0;
            let h = js_mod(12.0 + hue_shift + 360.0, 360.0);
            t.construct.push((
                key_obj(&[("l", 50.0), ("c", 80.0), ("h", h)]),
                index + 2,
            ));
        }
        let out = generate_team_colors(2, &t);
        assert_eq!(out.len(), 64);
        assert_eq!(out[0], 2);
        for (i, id) in out.iter().enumerate().skip(1) {
            assert_eq!(*id, (i + 2) as u32);
        }
        // buildTeamPalettes: Bot stays flat, order follows teamColors.
        let palettes = build_team_palettes(
            &[("Bot".to_string(), "#b0".to_string()), ("Red".to_string(), "#r0".to_string())],
            &t,
        );
        assert_eq!(palettes.len(), 2);
        assert_eq!(palettes[0].1, vec![1]);
        assert_eq!(palettes[1].1, out);
    }
}
