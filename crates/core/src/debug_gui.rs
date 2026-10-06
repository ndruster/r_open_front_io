//! Port of the `src/client/render/gl/debug/` GUI cluster — the lil-gui
//! folder/prop factories and the `buildTree` debug-tree layout — plus the
//! `LINES_PER_PLAYER` constant of `src/client/render/gl/passes/name-pass/
//! Types.ts` (the `CHAR_RANGE` / `MAX_CHARS` pair already lives in
//! [`crate::atlas_data`] / [`crate::text_layout`] and is NOT duplicated).
//!
//! lil-gui, `Controller` and `RenderSettings` are `import type` only in the
//! TS sources, so the runtime surface is pure data: the factories close over
//! a target object + key + a captured default snapshot, and the capture
//! observes them through a scripted mock GUI facade (the `draw` call records
//! the `add` / `addColor` / `name` / `load` trace).
//!
//! Faithfulness notes (quirk list):
//!
//! * `folder(label, children, opts = {})`: the default parameter fires ONLY
//!   on `undefined` (omitted or explicit); an explicit `null` throws the
//!   `opts.closed` TypeError (modelled as the `[1]` status token, and the
//!   capture scripts it). `opts.closed ?? true` falls back ONLY on nullish —
//!   `false`, `0` and `""` survive as RAW values (the `closed` field is
//!   dumped through the codec, so a `0` shows up as a number, not a bool).
//!   A non-object, non-nullish `opts` (number / string / bool / array) reads
//!   `.closed` through the wrapper as `undefined` → `true`.
//! * `toggle` / `slider` / `select`: `defaultVal = defaults[key]` is captured
//!   at construction (a missing key yields `undefined`); `isModified` is the
//!   STRICT `!==` — `NaN !== NaN` is true, `-0 !== 0` is false, a bool never
//!   equals a number. `resetToDefault` writes `target[key] = defaultVal`
//!   (an existing key keeps its slot, a missing key APPENDS — the final
//!   target dump pins the insertion order) and, through the mock facade,
//!   `ctrl.updateDisplay()` (a no-op with no data). The capture DOES call
//!   `draw(mockGui)`, so the `if (label) ctrl.name(label)` truthy gate is
//!   observable as the name trace, and the slider `min/max/step` and select
//!   `options` literals ride the `folder.add` call trace.
//! * `color`: the proxy is built at construction reading the target's three
//!   keys (observable through the mock's `addColor` reference); `isModified`
//!   is the three-way strict-`!==` OR; `resetToDefault` writes the three
//!   keys back to the captured defaults, rebuilds `proxy.color`, and calls
//!   `ctrl.load("#" + [dR,dG,dB].map(v => Math.round(v*255).toString(16)
//!   .padStart(2,"0")).join(""))` — the hex string is the hot quirk surface:
//!   - `Math.round` is JS half-UP toward +Infinity (`0.5 → 1`, `-0.5 → -0`,
//!     `-127.5 → -127`), applied AFTER the `v * 255` f64 multiply;
//!   - `Number#toString(16)`: lowercase hex for positive integers, `"-a"`
//!     form for negatives (so `padStart(2)` does NOT zero-pad them), `"0"`
//!     for both `+0` and `-0`, `"NaN"`, `"Infinity"` / `"-Infinity"`;
//!     integer-valued doubles above 2^53 print the EXACT hex of the double
//!     (V8's bigint radix path) — pinned by the golden;
//!   - `padStart(2,"0")` only pads when the UTF-16 length is below 2, so
//!     out-of-domain values produce odd strings like `"#NaNInfinity-7f"`.
//!     The hex is built from the CAPTURED defaults, never the live target.
//! * `buildTree(s, d)` is a pure literal tree: the only dynamic points are
//!   `Object.entries(s.structure.shapes)` and `Object.entries(s.lightConfigs)`
//!   (V8 own-key order, which the [`JsVal::Obj`] insertion order carries),
//!   with the values indexed through `d.structure.shapes[name]` /
//!   `d.lightConfigs[name]`. Every leaf factory call is transcribed verbatim.
//!   The capture dump walks the tree in order: `draw(mock)` → `isModified()`
//!   → `resetToDefault()` (which mutates `s` back to the defaults — the
//!   per-leaf order makes this deterministic and pins the captured snapshot).
//! * Strict `!==` on two distinct objects is true in JS; the codec compares
//!   `Obj` / `Arr` values as always-distinct (never equal) — the capture
//!   domain never puts objects into a prop slot anyway.

use crate::game_config_helpers::js_number;
use crate::js_json::{map_set, push_str, push_val, read_str, read_val, JsVal};
use crate::jsnum::js_round;

/// `LINES_PER_PLAYER` from `name-pass/Types.ts` (`0 = name, 1 = troop count`).
pub const LINES_PER_PLAYER: f64 = 2.0;

/// The four prop kinds; the dump tag is the discriminant + 1.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PropKind {
    Toggle,
    Slider,
    Select,
    Color,
}

impl PropKind {
    fn tag(self) -> f64 {
        match self {
            PropKind::Toggle => 1.0,
            PropKind::Slider => 2.0,
            PropKind::Select => 3.0,
            PropKind::Color => 4.0,
        }
    }
}

/// One leaf prop: the factory's captured state. `path` locates the target
/// container inside the `s` / `d` trees (empty = the root passed to the
/// lifecycle op); `snapshot` holds the `defaults[key]` value captured at
/// construction (`rgb_snapshot` for the three-key color prop).
#[derive(Clone, Debug, PartialEq)]
pub struct PropLeaf {
    pub kind: PropKind,
    pub path: Vec<String>,
    pub key: String,
    pub snapshot: JsVal,
    pub rgb_keys: [String; 3],
    pub rgb_snapshot: [JsVal; 3],
    /// The proxy's `{r,g,b}` as read from the TARGET at construction time.
    pub proxy_draw: [JsVal; 3],
    pub label: JsVal,
    pub min: f64,
    pub max: f64,
    pub step: f64,
    pub options: Vec<String>,
}

/// A debug-tree node: a folder or a prop leaf. The leaf is boxed to keep the
/// enum small (the folder variant is the hot one in the tree walk).
#[derive(Clone, Debug, PartialEq)]
pub enum DebugNode {
    Folder {
        label: String,
        closed: JsVal,
        children: Vec<DebugNode>,
    },
    Prop(Box<PropLeaf>),
}

// ------------------------------------------------------------------ JS-isms

/// JS `===` over the codec domain. `Absent` and `Undef` both model the
/// `undefined` a missing-key read yields. `Obj` / `Arr` are always distinct
/// (JS reference equality; the capture never stores objects in prop slots).
fn js_strict_eq(a: &JsVal, b: &JsVal) -> bool {
    match (a, b) {
        (JsVal::Num(x), JsVal::Num(y)) => *x == *y, // IEEE ==: NaN != NaN, -0 == 0
        (JsVal::Bool(x), JsVal::Bool(y)) => x == y,
        (JsVal::Str(x), JsVal::Str(y)) => x == y,
        (JsVal::Null, JsVal::Null) => true,
        (JsVal::Absent | JsVal::Undef, JsVal::Absent | JsVal::Undef) => true,
        _ => false,
    }
}

/// JS `!==`.
fn js_ne(a: &JsVal, b: &JsVal) -> bool {
    !js_strict_eq(a, b)
}

/// JS truthiness over the codec domain (`NaN`, `0`, `-0`, `""`, `false`,
/// `null`, `undefined` are falsy; objects / arrays truthy).
fn js_truthy(v: &JsVal) -> bool {
    match v {
        JsVal::Absent | JsVal::Undef | JsVal::Null | JsVal::Bool(false) => false,
        JsVal::Num(n) => !n.is_nan() && *n != 0.0,
        JsVal::Str(s) => !s.is_empty(),
        _ => true,
    }
}

/// JS `ToNumber` for the `v * 255` multiply. `Obj` / `Arr` would go through
/// `ToPrimitive`; the capture domain never feeds them and they are NaN here.
fn to_number(v: &JsVal) -> f64 {
    match v {
        JsVal::Num(n) => *n,
        JsVal::Bool(b) => {
            if *b {
                1.0
            } else {
                0.0
            }
        }
        JsVal::Null => 0.0,
        JsVal::Str(s) => js_number(s),
        _ => f64::NAN,
    }
}

/// Exact hex of an integer-valued double `>= 2^53` (the domain V8 reaches
/// through its bigint radix path): `value = m << e` with `m` the 53-bit
/// significand; emit the bit vector and regroup into nibbles.
fn big_hex(mag: f64) -> String {
    let bits = mag.to_bits();
    let exp = ((bits >> 52) & 0x7ff) as i32 - 1023;
    let frac = bits & ((1u64 << 52) - 1);
    let m: u128 = (1u128 << 52) | frac as u128;
    let e = (exp - 52) as usize; // >= 1 for mag >= 2^53
    let mut bv: Vec<u8> = (0..53).rev().map(|i| ((m >> i) & 1) as u8).collect();
    bv.extend(std::iter::repeat_n(0u8, e));
    let first1 = bv.iter().position(|b| *b == 1).unwrap();
    let body = &bv[first1..];
    let pad = (4 - body.len() % 4) % 4;
    let mut full: Vec<u8> = vec![0u8; pad];
    full.extend_from_slice(body);
    full.chunks(4)
        .map(|c| c.iter().fold(0u8, |a, b| a * 2 + b))
        .map(|n| char::from_digit(n as u32, 16).unwrap())
        .collect()
}

/// `Number#toString(16)` over the post-round domain: integers, `NaN`,
/// `±Infinity`, `±0`. Negative integers take the `"-a"` form (the minus
/// survives `padStart(2)` because the length is already >= 2).
pub fn to_hex_string(v: f64) -> String {
    if v.is_nan() {
        return "NaN".to_string();
    }
    if v == f64::INFINITY {
        return "Infinity".to_string();
    }
    if v == f64::NEG_INFINITY {
        return "-Infinity".to_string();
    }
    if v == 0.0 {
        return "0".to_string(); // +0 and -0 both spell "0"
    }
    let neg = v < 0.0;
    let mag = v.abs();
    let body = if mag < 9007199254740992.0 {
        format!("{:x}", mag as u64)
    } else {
        big_hex(mag)
    };
    if neg {
        format!("-{body}")
    } else {
        body
    }
}

/// `String#padStart(2, "0")` — the produced segments are ASCII, so the
/// byte-count gate is exact.
fn pad_start_2(s: &str) -> String {
    if s.len() < 2 {
        format!("0{s}")
    } else {
        s.to_string()
    }
}

/// `resetToDefault`'s `load` argument: `"#" + [dR,dG,dB].map(v =>
/// Math.round(v*255).toString(16).padStart(2,"0")).join("")` over the
/// CAPTURED defaults.
pub fn color_hex(defaults: &[JsVal]) -> String {
    let mut s = String::from("#");
    for v in defaults {
        let r = js_round(to_number(v) * 255.0);
        s.push_str(&pad_start_2(&to_hex_string(r)));
    }
    s
}

// ---------------------------------------------------------------- tree access

fn get_field<P: AsRef<str>>(root: &JsVal, path: &[P], key: &str) -> JsVal {
    let mut cur = root;
    for seg in path {
        let JsVal::Obj(fields) = cur else {
            return JsVal::Undef; // out of the capture domain (containers exist)
        };
        let Some((_, v)) = fields.iter().find(|(k, _)| k.as_str() == seg.as_ref()) else {
            return JsVal::Undef;
        };
        cur = v;
    }
    match cur {
        JsVal::Obj(fields) => fields
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
            .unwrap_or(JsVal::Undef),
        _ => JsVal::Undef,
    }
}

fn set_field(root: &mut JsVal, path: &[String], key: &str, v: JsVal) {
    let mut cur = root;
    for seg in path {
        let JsVal::Obj(fields) = cur else {
            unreachable!("debug_gui: container path {seg}");
        };
        cur = fields
            .iter_mut()
            .find(|(k, _)| k == seg)
            .map(|(_, x)| x)
            .unwrap();
    }
    let JsVal::Obj(fields) = cur else {
        unreachable!("debug_gui: set leaf container");
    };
    map_set(fields, key, v);
}

// -------------------------------------------------------------------- factory

/// `folder(label, children, opts)`'s `closed` computation. `None` models the
/// `opts.closed` TypeError on a nullish-`opts`-with-null value (explicit
/// `null` opts; `undefined` fires the default parameter instead).
pub fn folder_closed(opts: &JsVal) -> Option<JsVal> {
    match opts {
        // default parameter `opts = {}` -> `closed` undefined -> `?? true`.
        JsVal::Absent | JsVal::Undef => Some(JsVal::Bool(true)),
        JsVal::Null => None, // TypeError reading .closed off null
        JsVal::Obj(fields) => {
            let c = fields
                .iter()
                .find(|(k, _)| k == "closed")
                .map(|(_, v)| v.clone())
                .unwrap_or(JsVal::Undef);
            Some(match c {
                JsVal::Absent | JsVal::Undef | JsVal::Null => JsVal::Bool(true),
                other => other,
            })
        }
        // number / string / bool / array: the wrapper property read yields
        // undefined -> `?? true`.
        _ => Some(JsVal::Bool(true)),
    }
}

fn path_of(p: &[&str]) -> Vec<String> {
    p.iter().map(|s| s.to_string()).collect()
}

fn leaf_label(label: Option<&str>) -> JsVal {
    match label {
        None => JsVal::Absent,
        Some(l) => JsVal::Str(l.to_string()),
    }
}

fn tg(d: &JsVal, path: &[&str], key: &str, label: Option<&str>) -> DebugNode {
    DebugNode::Prop(Box::new(PropLeaf {
        kind: PropKind::Toggle,
        path: path_of(path),
        key: key.to_string(),
        snapshot: get_field(d, path, key),
        rgb_keys: Default::default(),
        rgb_snapshot: [JsVal::Undef, JsVal::Undef, JsVal::Undef],
        proxy_draw: [JsVal::Undef, JsVal::Undef, JsVal::Undef],
        label: leaf_label(label),
        min: 0.0,
        max: 0.0,
        step: 0.0,
        options: Vec::new(),
    }))
}

fn sl(
    d: &JsVal,
    path: &[&str],
    key: &str,
    min: f64,
    max: f64,
    step: f64,
    label: Option<&str>,
) -> DebugNode {
    DebugNode::Prop(Box::new(PropLeaf {
        kind: PropKind::Slider,
        path: path_of(path),
        key: key.to_string(),
        snapshot: get_field(d, path, key),
        rgb_keys: Default::default(),
        rgb_snapshot: [JsVal::Undef, JsVal::Undef, JsVal::Undef],
        proxy_draw: [JsVal::Undef, JsVal::Undef, JsVal::Undef],
        label: leaf_label(label),
        min,
        max,
        step,
        options: Vec::new(),
    }))
}

fn co(s: &JsVal, d: &JsVal, path: &[&str], r: &str, g: &str, b: &str, label: Option<&str>) -> DebugNode {
    DebugNode::Prop(Box::new(PropLeaf {
        kind: PropKind::Color,
        path: path_of(path),
        key: String::new(),
        snapshot: JsVal::Absent,
        rgb_keys: [r.to_string(), g.to_string(), b.to_string()],
        rgb_snapshot: [
            get_field(d, path, r),
            get_field(d, path, g),
            get_field(d, path, b),
        ],
        proxy_draw: [
            get_field(s, path, r),
            get_field(s, path, g),
            get_field(s, path, b),
        ],
        label: leaf_label(label),
        min: 0.0,
        max: 0.0,
        step: 0.0,
        options: Vec::new(),
    }))
}

fn fo(label: &str, children: Vec<DebugNode>) -> DebugNode {
    DebugNode::Folder {
        label: label.to_string(),
        closed: JsVal::Bool(true),
        children,
    }
}

impl PropLeaf {
    /// `isModified()` over the live `s` tree.
    pub fn is_modified(&self, s: &JsVal) -> bool {
        match self.kind {
            PropKind::Color => {
                js_ne(&get_field(s, &self.path, &self.rgb_keys[0]), &self.rgb_snapshot[0])
                    || js_ne(&get_field(s, &self.path, &self.rgb_keys[1]), &self.rgb_snapshot[1])
                    || js_ne(&get_field(s, &self.path, &self.rgb_keys[2]), &self.rgb_snapshot[2])
            }
            _ => js_ne(&get_field(s, &self.path, &self.key), &self.snapshot),
        }
    }

    /// `resetToDefault()`'s write-back into `s`.
    pub fn reset(&self, s: &mut JsVal) {
        match self.kind {
            PropKind::Color => {
                for i in 0..3 {
                    set_field(s, &self.path, &self.rgb_keys[i], self.rgb_snapshot[i].clone());
                }
            }
            _ => set_field(s, &self.path, &self.key, self.snapshot.clone()),
        }
    }
}

// ------------------------------------------------------------------ buildTree

/// `buildTree(s, d)` — the ~850-line literal tree transcribed verbatim. The
/// two dynamic `Object.entries` maps ride the insertion order of the parsed
/// `JsVal::Obj` trees (V8 own-key order).
pub fn build_tree(s: &JsVal, d: &JsVal) -> Vec<DebugNode> {
    let per_shape: Vec<DebugNode> = {
        let shapes = get_field(s, &["structure"], "shapes");
        let JsVal::Obj(fields) = &shapes else {
            unreachable!("debug_gui: structure.shapes must be an object");
        };
        fields
            .iter()
            .map(|(name, _cfg)| {
                let p = ["structure", "shapes", name.as_str()];
                fo(
                    name,
                    vec![
                        sl(d, &p, "scale", 0.5, 2.0, 0.05, Some("Frame Scale")),
                        sl(d, &p, "iconFill", 0.2, 1.5, 0.05, Some("Icon Fill")),
                    ],
                )
            })
            .collect()
    };
    let light_cfgs: Vec<DebugNode> = {
        let cfgs = get_field(s, &[] as &[&str], "lightConfigs");
        let JsVal::Obj(fields) = &cfgs else {
            unreachable!("debug_gui: lightConfigs must be an object");
        };
        fields
            .iter()
            .map(|(name, _cfg)| {
                let p = ["lightConfigs", name.as_str()];
                fo(
                    name,
                    vec![
                        sl(d, &p, "radius", 1.0, 60.0, 1.0, None),
                        sl(d, &p, "intensity", 0.0, 10.0, 0.1, None),
                    ],
                )
            })
            .collect()
    };

    vec![
        fo(
            "Pass Enables",
            vec![
                tg(d, &["passEnabled"], "terrain", None),
                tg(d, &["passEnabled"], "territory", None),
                tg(d, &["passEnabled"], "borderCompute", None),
                tg(d, &["passEnabled"], "borderStamp", None),
                tg(d, &["passEnabled"], "trail", None),
                tg(d, &["passEnabled"], "structure", None),
                tg(d, &["passEnabled"], "unit", None),
                tg(d, &["passEnabled"], "name", None),
                tg(d, &["passEnabled"], "falloutBloom", None),
                tg(d, &["passEnabled"], "railroad", None),
                tg(d, &["passEnabled"], "fx", None),
                tg(d, &["passEnabled"], "bar", None),
                tg(d, &["passEnabled"], "nameDebug", Some("Name Debug Boxes")),
            ],
        ),
        fo(
            "Fallout Bloom",
            vec![
                sl(d, &["falloutBloom"], "broilSpeedCold", 0.0, 0.05, 0.0001, None),
                sl(d, &["falloutBloom"], "broilSpeedHot", 0.0, 0.05, 0.0001, None),
                sl(d, &["falloutBloom"], "noiseFreq1", 0.0, 0.5, 0.001, None),
                sl(d, &["falloutBloom"], "noiseFreq2", 0.0, 0.5, 0.001, None),
                sl(d, &["falloutBloom"], "contrastLoCold", 0.0, 1.0, 0.01, None),
                sl(d, &["falloutBloom"], "contrastLoHot", 0.0, 1.0, 0.01, None),
                sl(d, &["falloutBloom"], "contrastHiCold", 0.0, 1.0, 0.01, None),
                sl(d, &["falloutBloom"], "contrastHiHot", 0.0, 1.0, 0.01, None),
                sl(d, &["falloutBloom"], "metaFreq", 0.0, 0.2, 0.001, None),
                sl(d, &["falloutBloom"], "intensityCold", 0.0, 10.0, 0.05, None),
                sl(d, &["falloutBloom"], "intensityHot", 0.0, 20.0, 0.1, None),
                sl(d, &["falloutBloom"], "metaInfluenceCold", 0.0, 1.0, 0.01, None),
                sl(d, &["falloutBloom"], "metaInfluenceHot", 0.0, 1.0, 0.01, None),
                sl(d, &["falloutBloom"], "opacityFadeEnd", 0.0, 1.0, 0.01, None),
                co(
                    s,
                    d,
                    &["falloutBloom"],
                    "bloomR",
                    "bloomG",
                    "bloomB",
                    Some("Bloom Color"),
                ),
                sl(d, &["falloutBloom"], "bloomCoverage", 0.0, 10.0, 0.1, None),
                sl(d, &["falloutBloom"], "heatDecayPerTick", 0.0, 5.0, 0.01, None),
                co(
                    s,
                    d,
                    &["falloutBloom"],
                    "particleColorDarkR",
                    "particleColorDarkG",
                    "particleColorDarkB",
                    Some("Particle Color Dark"),
                ),
                co(
                    s,
                    d,
                    &["falloutBloom"],
                    "particleColorBrightR",
                    "particleColorBrightG",
                    "particleColorBrightB",
                    Some("Particle Color Bright"),
                ),
                sl(
                    d,
                    &["falloutBloom"],
                    "particleThresholdUnowned",
                    0.5,
                    1.0,
                    0.005,
                    None,
                ),
                sl(
                    d,
                    &["falloutBloom"],
                    "particleThresholdOwned",
                    0.5,
                    1.0,
                    0.005,
                    None,
                ),
                sl(d, &["falloutBloom"], "particleFlickerSpeed", 0.0, 2.0, 0.01, None),
                sl(d, &["falloutBloom"], "particleStrength", 0.0, 5.0, 0.01, None),
                sl(d, &["falloutBloom"], "particleFreshScale", 0.0, 1.0, 0.01, None),
            ],
        ),
        fo(
            "Lighting",
            vec![
                tg(d, &["lighting"], "enabled", None),
                sl(d, &["lighting"], "ambient", 0.0, 1.0, 0.01, None),
                sl(d, &["lighting"], "falloffPower", 0.5, 5.0, 0.1, None),
                sl(d, &["lighting"], "falloutLightIntensity", 0.0, 20.0, 0.1, None),
                sl(d, &["lighting"], "falloutLightThreshold", 0.0, 0.5, 0.001, None),
                sl(d, &["lighting"], "blurZoomDivisor", 1.0, 20.0, 0.5, None),
                sl(d, &["lighting"], "lightRadiusMultiplier", 0.1, 5.0, 0.1, None),
                co(
                    s,
                    d,
                    &["lighting"],
                    "falloutLightR",
                    "falloutLightG",
                    "falloutLightB",
                    Some("Fallout Light Color"),
                ),
                sl(d, &["lighting"], "emberLightIntensity", 0.0, 20.0, 0.1, None),
                co(
                    s,
                    d,
                    &["lighting"],
                    "emberLightR",
                    "emberLightG",
                    "emberLightB",
                    Some("Ember Light Color"),
                ),
            ],
        ),
        fo(
            "Map Overlay",
            vec![
                sl(d, &["mapOverlay"], "trailAlpha", 0.0, 1.0, 0.01, None),
                sl(d, &["mapOverlay"], "defenseCheckerDarken", 0.0, 1.0, 0.01, None),
                sl(d, &["mapOverlay"], "territoryDefenseDarken", 0.0, 1.0, 0.01, None),
                sl(
                    d,
                    &["mapOverlay"],
                    "territorySaturation",
                    0.0,
                    1.0,
                    0.01,
                    Some("Territory Saturation"),
                ),
                sl(
                    d,
                    &["mapOverlay"],
                    "territoryAlpha",
                    0.0,
                    1.0,
                    0.01,
                    Some("Territory Alpha"),
                ),
                sl(d, &["mapOverlay"], "staleNukeBase", 0.0, 0.3, 0.005, None),
                sl(d, &["mapOverlay"], "staleNukeVariation", 0.0, 0.3, 0.005, None),
                sl(d, &["mapOverlay"], "staleNukeAlpha", 0.0, 1.0, 0.01, None),
                co(
                    s,
                    d,
                    &["mapOverlay"],
                    "staleNukeR",
                    "staleNukeG",
                    "staleNukeB",
                    Some("Stale Nuke Color"),
                ),
                sl(
                    d,
                    &["mapOverlay"],
                    "highlightBrighten",
                    0.0,
                    1.0,
                    0.01,
                    Some("Highlight Brighten (border)"),
                ),
                sl(
                    d,
                    &["mapOverlay"],
                    "highlightFillBrighten",
                    0.0,
                    1.0,
                    0.01,
                    Some("Highlight Brighten (fill)"),
                ),
                sl(
                    d,
                    &["mapOverlay"],
                    "highlightThicken",
                    0.0,
                    10.0,
                    1.0,
                    Some("Highlight Thicken (tiles)"),
                ),
                fo(
                    "Railroad",
                    vec![
                        sl(d, &["railroad"], "railMinZoom", 0.0, 10.0, 0.1, Some("Min Zoom")),
                        sl(d, &["railroad"], "railFadeRange", 0.0, 5.0, 0.1, Some("Fade Range")),
                        sl(
                            d,
                            &["railroad"],
                            "railDetailZoom",
                            0.0,
                            20.0,
                            0.1,
                            Some("Detail Zoom"),
                        ),
                        sl(d, &["railroad"], "railAlpha", 0.0, 1.0, 0.01, Some("Alpha")),
                        sl(
                            d,
                            &["railroad"],
                            "railThickness",
                            0.5,
                            3.0,
                            0.1,
                            Some("Thickness"),
                        ),
                    ],
                ),
            ],
        ),
        fo(
            "Structure",
            vec![
                sl(d, &["structure"], "iconSize", 10.0, 100.0, 1.0, None),
                sl(d, &["structure"], "dotsZoomThreshold", 0.1, 2.0, 0.05, None),
                sl(
                    d,
                    &["structure"],
                    "iconScaleFactorZoomedOut",
                    0.5,
                    3.0,
                    0.05,
                    None,
                ),
                sl(
                    d,
                    &["structure"],
                    "highlightOutlineWidth",
                    0.0,
                    0.2,
                    0.005,
                    Some("Highlight Outline W"),
                ),
                sl(
                    d,
                    &["structure"],
                    "highlightDimAlpha",
                    0.0,
                    1.0,
                    0.01,
                    Some("Highlight Dim Alpha"),
                ),
                fo("Per-Shape", per_shape),
            ],
        ),
        fo(
            "Structure Level",
            vec![
                sl(d, &["structureLevel"], "scale", 0.5, 3.0, 0.05, Some("Scale")),
                sl(
                    d,
                    &["structureLevel"],
                    "outlineWidth",
                    0.0,
                    20.0,
                    0.1,
                    Some("Outline Width (px)"),
                ),
                sl(
                    d,
                    &["structureLevel"],
                    "offsetY",
                    -2.0,
                    2.0,
                    0.05,
                    Some("Height Above Icon"),
                ),
            ],
        ),
        fo(
            "Bar",
            vec![
                sl(d, &["bar"], "healthBarW", 3.0, 30.0, 1.0, Some("Health Width")),
                sl(d, &["bar"], "healthBarH", 1.0, 10.0, 1.0, Some("Health Height")),
                sl(
                    d,
                    &["bar"],
                    "healthBarOffsetY",
                    -20.0,
                    0.0,
                    1.0,
                    Some("Health Offset Y"),
                ),
                sl(
                    d,
                    &["bar"],
                    "progressBarW",
                    3.0,
                    30.0,
                    1.0,
                    Some("Progress Width"),
                ),
                sl(
                    d,
                    &["bar"],
                    "progressBarH",
                    1.0,
                    10.0,
                    1.0,
                    Some("Progress Height"),
                ),
                sl(
                    d,
                    &["bar"],
                    "progressBarOffsetY",
                    0.0,
                    20.0,
                    1.0,
                    Some("Progress Offset Y"),
                ),
                sl(d, &["bar"], "borderWidth", 0.0, 3.0, 0.5, Some("Border Width")),
                sl(d, &["bar"], "threshold1", 0.0, 1.0, 0.05, Some("Red→Orange")),
                sl(d, &["bar"], "threshold2", 0.0, 1.0, 0.05, Some("Orange→Yellow")),
                sl(d, &["bar"], "threshold3", 0.0, 1.0, 0.05, Some("Yellow→Green")),
                co(s, d, &["bar"], "colorRedR", "colorRedG", "colorRedB", Some("Red")),
                co(
                    s,
                    d,
                    &["bar"],
                    "colorOrangeR",
                    "colorOrangeG",
                    "colorOrangeB",
                    Some("Orange"),
                ),
                co(
                    s,
                    d,
                    &["bar"],
                    "colorYellowR",
                    "colorYellowG",
                    "colorYellowB",
                    Some("Yellow"),
                ),
                co(
                    s,
                    d,
                    &["bar"],
                    "colorGreenR",
                    "colorGreenG",
                    "colorGreenB",
                    Some("Green"),
                ),
            ],
        ),
        fo(
            "Unit",
            vec![
                sl(d, &["unit"], "unitSize", 4.0, 64.0, 1.0, None),
                sl(d, &["unit"], "flickerSpeed", 0.0, 2.0, 0.01, None),
                co(s, d, &["unit"], "angryR", "angryG", "angryB", Some("Angry Color")),
            ],
        ),
        fo(
            "Name",
            vec![
                sl(d, &["name"], "lerpSpeed", 1.0, 30.0, 0.5, None),
                sl(d, &["name"], "cullThreshold", 0.0, 0.05, 0.001, None),
                sl(d, &["name"], "nameScaleFactor", 0.1, 1.0, 0.05, None),
                sl(d, &["name"], "nameScaleCap", 1.0, 10.0, 0.5, None),
                sl(d, &["name"], "troopSizeMultiplier", 0.1, 2.0, 0.05, None),
                sl(
                    d,
                    &["name"],
                    "outlineWidth",
                    0.0,
                    10.0,
                    0.1,
                    Some("Outline Width (px)"),
                ),
                co(
                    s,
                    d,
                    &["name"],
                    "outlineR",
                    "outlineG",
                    "outlineB",
                    Some("Outline Color"),
                ),
                tg(
                    d,
                    &["name"],
                    "outlineUsePlayerColor",
                    Some("Outline = Player Color"),
                ),
                tg(
                    d,
                    &["name"],
                    "fillUsePlayerColor",
                    Some("Fill = Player Color"),
                ),
                sl(
                    d,
                    &["name"],
                    "emojiRowOffset",
                    0.0,
                    5.0,
                    0.1,
                    Some("Emoji Row Offset"),
                ),
                sl(
                    d,
                    &["name"],
                    "statusRowOffset",
                    0.0,
                    5.0,
                    0.1,
                    Some("Status Row Offset"),
                ),
                sl(
                    d,
                    &["name"],
                    "statusOutlineWidth",
                    0.0,
                    16.0,
                    0.5,
                    Some("Status Outline Width"),
                ),
                sl(
                    d,
                    &["name"],
                    "hoverFadeAlpha",
                    0.0,
                    1.0,
                    0.05,
                    Some("Hover Fade Alpha"),
                ),
                sl(
                    d,
                    &["name"],
                    "hoverGlowWidth",
                    0.0,
                    8.0,
                    0.25,
                    Some("Hover Glow Width"),
                ),
                sl(
                    d,
                    &["name"],
                    "hoverGlowAlpha",
                    0.0,
                    1.0,
                    0.05,
                    Some("Hover Glow Alpha"),
                ),
            ],
        ),
        fo(
            "FX",
            vec![
                sl(d, &["fx"], "shockwaveRingWidth", 0.01, 0.2, 0.005, None),
                sl(
                    d,
                    &["fx"],
                    "attackRingScreenPx",
                    5.0,
                    60.0,
                    1.0,
                    Some("Attack Ring Size (px)"),
                ),
                sl(
                    d,
                    &["fx"],
                    "nukeShockwaveDurationMs",
                    200.0,
                    5000.0,
                    100.0,
                    Some("Nuke Shock Duration"),
                ),
                sl(
                    d,
                    &["fx"],
                    "nukeShockwaveRadiusFactor",
                    0.5,
                    3.0,
                    0.1,
                    Some("Nuke Shock Radius ×"),
                ),
                sl(
                    d,
                    &["fx"],
                    "samShockwaveDurationMs",
                    200.0,
                    3000.0,
                    50.0,
                    Some("SAM Shock Duration"),
                ),
                sl(
                    d,
                    &["fx"],
                    "samShockwaveRadius",
                    10.0,
                    100.0,
                    5.0,
                    Some("SAM Shock Radius"),
                ),
                sl(
                    d,
                    &["fx"],
                    "debrisLifetimeMs",
                    1000.0,
                    15000.0,
                    500.0,
                    Some("Debris Lifetime"),
                ),
                sl(
                    d,
                    &["fx"],
                    "debrisFadeIn",
                    0.0,
                    0.5,
                    0.01,
                    Some("Debris Fade In"),
                ),
                sl(
                    d,
                    &["fx"],
                    "debrisFadeOut",
                    0.3,
                    1.0,
                    0.01,
                    Some("Debris Fade Out"),
                ),
                sl(
                    d,
                    &["fx"],
                    "conquestLifetimeMs",
                    500.0,
                    8000.0,
                    250.0,
                    Some("Conquest Lifetime"),
                ),
                sl(
                    d,
                    &["fx"],
                    "conquestFadeIn",
                    0.0,
                    0.5,
                    0.01,
                    Some("Conquest Fade In"),
                ),
                sl(
                    d,
                    &["fx"],
                    "conquestFadeOut",
                    0.3,
                    1.0,
                    0.01,
                    Some("Conquest Fade Out"),
                ),
                sl(
                    d,
                    &["fx"],
                    "nukeRadiusAtom",
                    10.0,
                    400.0,
                    5.0,
                    Some("Atom Bomb Radius"),
                ),
                sl(
                    d,
                    &["fx"],
                    "nukeRadiusHydro",
                    10.0,
                    400.0,
                    5.0,
                    Some("Hydrogen Bomb Radius"),
                ),
                sl(
                    d,
                    &["fx"],
                    "nukeRadiusMirv",
                    10.0,
                    400.0,
                    5.0,
                    Some("MIRV Warhead Radius"),
                ),
                sl(
                    d,
                    &["fx"],
                    "debrisDensity",
                    0.0,
                    4.0,
                    0.1,
                    Some("Debris Density ×"),
                ),
            ],
        ),
        fo(
            "Nuke Trajectory",
            vec![
                sl(
                    d,
                    &["nukeTrajectory"],
                    "lineWidth",
                    0.5,
                    5.0,
                    0.25,
                    Some("Line Width (px)"),
                ),
                sl(
                    d,
                    &["nukeTrajectory"],
                    "outlineWidth",
                    0.0,
                    4.0,
                    0.25,
                    Some("Outline Width (px)"),
                ),
                sl(
                    d,
                    &["nukeTrajectory"],
                    "dashTargetable",
                    1.0,
                    30.0,
                    1.0,
                    Some("Dash (targetable)"),
                ),
                sl(
                    d,
                    &["nukeTrajectory"],
                    "gapTargetable",
                    1.0,
                    20.0,
                    1.0,
                    Some("Gap (targetable)"),
                ),
                sl(
                    d,
                    &["nukeTrajectory"],
                    "dashUntargetable",
                    1.0,
                    20.0,
                    1.0,
                    Some("Dash (untargetable)"),
                ),
                sl(
                    d,
                    &["nukeTrajectory"],
                    "gapUntargetable",
                    1.0,
                    20.0,
                    1.0,
                    Some("Gap (untargetable)"),
                ),
                co(
                    s,
                    d,
                    &["nukeTrajectory"],
                    "lineR",
                    "lineG",
                    "lineB",
                    Some("Line Color"),
                ),
                co(
                    s,
                    d,
                    &["nukeTrajectory"],
                    "interceptR",
                    "interceptG",
                    "interceptB",
                    Some("Intercept Color"),
                ),
                co(
                    s,
                    d,
                    &["nukeTrajectory"],
                    "outlineR",
                    "outlineG",
                    "outlineB",
                    Some("Outline Color"),
                ),
                co(
                    s,
                    d,
                    &["nukeTrajectory"],
                    "interceptOutlineR",
                    "interceptOutlineG",
                    "interceptOutlineB",
                    Some("Intercept Outline"),
                ),
                sl(
                    d,
                    &["nukeTrajectory"],
                    "markerCircleRadius",
                    2.0,
                    16.0,
                    1.0,
                    Some("Circle Marker (px)"),
                ),
                sl(
                    d,
                    &["nukeTrajectory"],
                    "markerXRadius",
                    2.0,
                    64.0,
                    1.0,
                    Some("X Marker (px)"),
                ),
            ],
        ),
        fo(
            "Nuke Telegraph",
            vec![
                sl(
                    d,
                    &["nukeTelegraph"],
                    "strokeWidth",
                    0.5,
                    5.0,
                    0.25,
                    Some("Stroke Width"),
                ),
                sl(
                    d,
                    &["nukeTelegraph"],
                    "dashLen",
                    2.0,
                    30.0,
                    1.0,
                    Some("Dash Length"),
                ),
                sl(
                    d,
                    &["nukeTelegraph"],
                    "gapLen",
                    1.0,
                    20.0,
                    1.0,
                    Some("Gap Length"),
                ),
                sl(
                    d,
                    &["nukeTelegraph"],
                    "rotationSpeed",
                    0.0,
                    60.0,
                    1.0,
                    Some("Rotation Speed"),
                ),
                sl(
                    d,
                    &["nukeTelegraph"],
                    "baseAlpha",
                    0.0,
                    1.0,
                    0.05,
                    Some("Base Alpha"),
                ),
                sl(
                    d,
                    &["nukeTelegraph"],
                    "pulseAmplitude",
                    0.0,
                    0.5,
                    0.01,
                    Some("Pulse Amplitude"),
                ),
                sl(
                    d,
                    &["nukeTelegraph"],
                    "pulseSpeed",
                    0.0,
                    10.0,
                    0.5,
                    Some("Pulse Speed"),
                ),
                sl(
                    d,
                    &["nukeTelegraph"],
                    "fillAlphaOffset",
                    0.0,
                    1.0,
                    0.05,
                    Some("Fill Alpha Offset"),
                ),
                co(
                    s,
                    d,
                    &["nukeTelegraph"],
                    "colorR",
                    "colorG",
                    "colorB",
                    Some("Color"),
                ),
            ],
        ),
        fo(
            "Move Indicator",
            vec![
                sl(
                    d,
                    &["moveIndicator"],
                    "startRadius",
                    1.0,
                    40.0,
                    1.0,
                    Some("Start Radius (px)"),
                ),
                sl(
                    d,
                    &["moveIndicator"],
                    "chevronSize",
                    1.0,
                    20.0,
                    0.5,
                    Some("Chevron Size (px)"),
                ),
                sl(
                    d,
                    &["moveIndicator"],
                    "lineWidth",
                    0.5,
                    6.0,
                    0.25,
                    Some("Line Width (px)"),
                ),
                sl(
                    d,
                    &["moveIndicator"],
                    "duration",
                    100.0,
                    3000.0,
                    50.0,
                    Some("Duration (ms)"),
                ),
                sl(
                    d,
                    &["moveIndicator"],
                    "converge",
                    0.0,
                    1.0,
                    0.05,
                    Some("Converge"),
                ),
            ],
        ),
        fo(
            "SAM Radius",
            vec![
                sl(
                    d,
                    &["samRadius"],
                    "strokeWidth",
                    0.5,
                    5.0,
                    0.1,
                    Some("Stroke Width"),
                ),
                sl(d, &["samRadius"], "dashLen", 2.0, 30.0, 1.0, Some("Dash Length")),
                sl(d, &["samRadius"], "gapLen", 1.0, 20.0, 1.0, Some("Gap Length")),
                sl(
                    d,
                    &["samRadius"],
                    "rotationSpeed",
                    0.0,
                    40.0,
                    1.0,
                    Some("Rotation Speed"),
                ),
                sl(d, &["samRadius"], "alpha", 0.0, 1.0, 0.05, Some("Alpha")),
                sl(
                    d,
                    &["samRadius"],
                    "outlineWidth",
                    0.0,
                    2.0,
                    0.05,
                    Some("Outline Width"),
                ),
                sl(
                    d,
                    &["samRadius"],
                    "outlineSoftness",
                    0.0,
                    1.0,
                    0.05,
                    Some("Outline Softness"),
                ),
            ],
        ),
        fo(
            "Bonus Popup",
            vec![
                sl(d, &["bonusPopup"], "scale", 1.0, 12.0, 0.5, Some("Scale")),
                sl(
                    d,
                    &["bonusPopup"],
                    "lifetimeMs",
                    500.0,
                    5000.0,
                    100.0,
                    Some("Lifetime (ms)"),
                ),
                sl(
                    d,
                    &["bonusPopup"],
                    "riseSpeed",
                    0.0,
                    10.0,
                    0.5,
                    Some("Rise Speed"),
                ),
                sl(
                    d,
                    &["bonusPopup"],
                    "yOffset",
                    -10.0,
                    10.0,
                    0.5,
                    Some("Y Offset"),
                ),
                sl(
                    d,
                    &["bonusPopup"],
                    "outlineWidth",
                    0.0,
                    5.0,
                    0.1,
                    Some("Outline Width"),
                ),
                co(
                    s,
                    d,
                    &["bonusPopup"],
                    "colorR",
                    "colorG",
                    "colorB",
                    Some("Color"),
                ),
                sl(
                    d,
                    &["bonusPopup"],
                    "minScreenScale",
                    0.0,
                    1.0,
                    0.01,
                    Some("Min Screen Scale"),
                ),
                sl(d, &["bonusPopup"], "cullZoom", 0.0, 2.0, 0.05, Some("Cull Zoom")),
            ],
        ),
        fo(
            "Spawn Overlay",
            vec![
                sl(
                    d,
                    &["spawnOverlay"],
                    "highlightRadius",
                    1.0,
                    20.0,
                    1.0,
                    Some("Highlight Radius"),
                ),
                sl(
                    d,
                    &["spawnOverlay"],
                    "highlightAlpha",
                    0.0,
                    1.0,
                    0.05,
                    Some("Highlight Alpha"),
                ),
                sl(
                    d,
                    &["spawnOverlay"],
                    "selfMinRad",
                    1.0,
                    30.0,
                    0.5,
                    Some("Self Min Radius"),
                ),
                sl(
                    d,
                    &["spawnOverlay"],
                    "selfMaxRad",
                    5.0,
                    50.0,
                    0.5,
                    Some("Self Max Radius"),
                ),
                sl(
                    d,
                    &["spawnOverlay"],
                    "mateMinRad",
                    1.0,
                    20.0,
                    0.5,
                    Some("Mate Min Radius"),
                ),
                sl(
                    d,
                    &["spawnOverlay"],
                    "mateMaxRad",
                    5.0,
                    30.0,
                    0.5,
                    Some("Mate Max Radius"),
                ),
                sl(
                    d,
                    &["spawnOverlay"],
                    "animSpeed",
                    0.001,
                    0.02,
                    0.001,
                    Some("Anim Speed"),
                ),
                sl(
                    d,
                    &["spawnOverlay"],
                    "gradientInnerEdge",
                    0.001,
                    0.1,
                    0.001,
                    Some("Gradient Inner Edge"),
                ),
                sl(
                    d,
                    &["spawnOverlay"],
                    "gradientSolidEnd",
                    0.01,
                    0.5,
                    0.01,
                    Some("Gradient Solid End"),
                ),
            ],
        ),
        fo(
            "Alt View",
            vec![
                sl(
                    d,
                    &["altView"],
                    "gridFontSize",
                    6.0,
                    32.0,
                    1.0,
                    Some("Grid Font Size"),
                ),
                tg(
                    d,
                    &["altView"],
                    "recolorStructures",
                    Some("Recolor Structures"),
                ),
                sl(d, &["altView"], "fillAlpha", 0.0, 1.0, 0.01, Some("Fill Alpha")),
            ],
        ),
        fo("Light Configs", light_cfgs),
    ]
}

// ------------------------------------------------------------------ dump walk

fn push_name(out: &mut Vec<f64>, label: &JsVal) {
    if js_truthy(label) {
        push_val(out, label);
    } else {
        out.push(0.0); // Absent marker: ctrl.name never called
    }
}

/// The buildTree leaf dump: draw(mock) -> name trace -> (color: proxy@draw)
/// -> isModified -> resetToDefault -> (color: hex + proxy@reset) -> the
/// post-reset target read -> the inert factory literals.
fn push_leaf(out: &mut Vec<f64>, p: &PropLeaf, s: &mut JsVal) {
    out.push(p.kind.tag());
    push_name(out, &p.label);
    if p.kind == PropKind::Color {
        for x in &p.proxy_draw {
            push_val(out, x);
        }
    }
    out.push(if p.is_modified(s) { 1.0 } else { 0.0 });
    match p.kind {
        PropKind::Color => {
            push_str(out, &color_hex(&p.rgb_snapshot));
            p.reset(s);
            // proxy.color after resetToDefault === the captured defaults.
            for x in &p.rgb_snapshot {
                push_val(out, x);
            }
        }
        PropKind::Slider => {
            p.reset(s);
            push_val(out, &get_field(s, &p.path, &p.key));
            out.push(p.min);
            out.push(p.max);
            out.push(p.step);
        }
        PropKind::Select => {
            p.reset(s);
            push_val(out, &get_field(s, &p.path, &p.key));
            out.push(p.options.len() as f64);
            for o in &p.options {
                push_str(out, o);
            }
        }
        PropKind::Toggle => {
            p.reset(s);
            push_val(out, &get_field(s, &p.path, &p.key));
        }
    }
}

fn push_tree(out: &mut Vec<f64>, nodes: &[DebugNode], s: &mut JsVal) {
    for n in nodes {
        match n {
            DebugNode::Folder {
                label,
                closed,
                children,
            } => {
                out.push(0.0);
                push_str(out, label);
                push_val(out, closed);
                out.push(children.len() as f64);
                push_tree(out, children, s);
            }
            DebugNode::Prop(p) => push_leaf(out, p, s),
        }
    }
}

// ---------------------------------------------------------------- vectors op
//
// kind 0: folder factory batch
//   args [n, (encS label, encVal opts, k, (encVal child)*k)*n] ->
//   per item [status 0|1, encS label, encVal closed, k, (encVal child)*k]
//   (status 1 models the `opts.closed` TypeError on an explicit null opts)
// kind 1: toggle lifecycle
//   args [encVal target, encVal key, encVal defaults, encVal label,
//         m, (encVal mut)*m] ->
//   [0, 1, encVal nameTrace|0, im_0..im_m, encVal target[key] after reset,
//    encVal target] | [1] (TypeError: nullish target/defaults)
// kind 2: slider lifecycle - same wire, leaf tag 2, plus [min, max, step]
//   between the after-reset read and the target dump.
// kind 3: select lifecycle - same wire, leaf tag 3, plus
//   [n_options, (encS option)*n].
// kind 4: color lifecycle
//   args [encVal target, encVal rKey, encVal gKey, encVal bKey,
//         encVal defaults, encVal label, m, (encVal r, encVal g, encVal b)*m]
//   -> [0, 4, encVal nameTrace|0, (encVal proxy@draw)*3, im_0..im_m,
//       encS hex, (encVal proxy@reset)*3, encVal target]
//   | [1] (TypeError: nullish target/defaults - the proxy construction or
//   the first isModified read throws)
// kind 5: buildTree dump
//   args [encVal s, encVal d, m, (n_path, (encS seg)*n_path, encS key,
//         encVal value)*m] -> [n, (dump node)*n] - the muts are applied to s
//   before the build, the walk draws + resets every leaf in tree order.
// kind 6: constants -> [LINES_PER_PLAYER]

fn key_str(k: &JsVal) -> String {
    match k {
        JsVal::Str(s) => s.clone(),
        JsVal::Undef | JsVal::Absent => "undefined".to_string(),
        JsVal::Null => "null".to_string(),
        JsVal::Bool(b) => if *b { "true" } else { "false" }.to_string(),
        JsVal::Num(n) => crate::js_fixed::js_to_string(*n),
        _ => "NaN".to_string(), // Obj/Arr: ToPropertyKey domain, never scripted
    }
}

/// Shared lifecycle runner for the single-key props (kinds 1-3). `tag` is the
/// leaf tag; `extra` is appended after the after-reset read (slider range
/// literals / select options) but before the final target dump.
fn run_single_prop(
    out: &mut Vec<f64>,
    tag: f64,
    args: &[f64],
    i: &mut usize,
    extra: &[f64],
) {
    let mut target = read_val(args, i);
    let key = key_str(&read_val(args, i));
    let defaults = read_val(args, i);
    let label = read_val(args, i);
    // Nullish defaults throws at construction (`defaults[key]`); a
    // non-object target throws in the strict-mode write-back (or the
    // nullish read) - the capture discards the partial output and records
    // the [1] status either way.
    if matches!(defaults, JsVal::Null | JsVal::Undef | JsVal::Absent)
        || !matches!(target, JsVal::Obj(_))
    {
        out.push(1.0);
        return;
    }
    let default_val = match &defaults {
        JsVal::Obj(fields) => fields
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, v)| v.clone())
            .unwrap_or(JsVal::Undef),
        _ => JsVal::Undef,
    };
    let m = args[*i] as usize;
    *i += 1;
    let mut muts = Vec::with_capacity(m);
    for _ in 0..m {
        muts.push(read_val(args, i));
    }
    let read_t = |t: &JsVal| -> JsVal {
        match t {
            JsVal::Obj(fields) => fields
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| v.clone())
                .unwrap_or(JsVal::Undef),
            _ => JsVal::Undef,
        }
    };
    out.push(0.0);
    out.push(tag);
    push_name(out, &label);
    out.push(if js_ne(&read_t(&target), &default_val) { 1.0 } else { 0.0 });
    for mu in &muts {
        let JsVal::Obj(fields) = &mut target else {
            unreachable!("debug_gui: lifecycle target object");
        };
        map_set(fields, &key, mu.clone());
        out.push(if js_ne(&read_t(&target), &default_val) { 1.0 } else { 0.0 });
    }
    // resetToDefault: target[key] = defaultVal (+ the mock updateDisplay
    // no-op).
    let JsVal::Obj(fields) = &mut target else {
        unreachable!("debug_gui: lifecycle reset target");
    };
    map_set(fields, &key, default_val);
    push_val(out, &read_t(&target));
    out.extend_from_slice(extra);
    push_val(out, &target);
}

pub fn run_op(kind: u8, args: &[f64]) -> Vec<f64> {
    let mut out = Vec::new();
    let mut i = 0usize;
    match kind {
        0 => {
            let n = args[i] as usize;
            i += 1;
            for _ in 0..n {
                let label = read_str(args, &mut i);
                let opts = read_val(args, &mut i);
                let k = args[i] as usize;
                i += 1;
                let mut children = Vec::with_capacity(k);
                for _ in 0..k {
                    children.push(read_val(args, &mut i));
                }
                match folder_closed(&opts) {
                    Some(closed) => {
                        out.push(0.0);
                        push_str(&mut out, &label);
                        push_val(&mut out, &closed);
                        out.push(k as f64);
                        for c in &children {
                            push_val(&mut out, c);
                        }
                    }
                    None => out.push(1.0),
                }
            }
        }
        1 => run_single_prop(&mut out, 1.0, args, &mut i, &[]),
        2 => {
            let min = args[i];
            let max = args[i + 1];
            let step = args[i + 2];
            // The range literals ride the args stream BEFORE the target
            // (the TS factory argument order is target, key, defaults, min,
            // max, step, label); the capture encodes them raw.
            i += 3;
            let mut inner = Vec::new();
            run_single_prop(&mut inner, 2.0, args, &mut i, &[min, max, step]);
            out.extend(inner);
        }
        3 => {
            let no = args[i] as usize;
            i += 1;
            let mut opts_enc: Vec<f64> = Vec::new();
            for _ in 0..no {
                let s = read_str(args, &mut i);
                push_str(&mut opts_enc, &s);
            }
            let mut inner = Vec::new();
            // The options dump lands AFTER the after-reset read, so splice it
            // in through the extra slice; the count goes first.
            let mut extra = vec![no as f64];
            extra.append(&mut opts_enc);
            run_single_prop(&mut inner, 3.0, args, &mut i, &extra);
            out.extend(inner);
        }
        4 => {
            let mut target = read_val(args, &mut i);
            let rkey = key_str(&read_val(args, &mut i));
            let gkey = key_str(&read_val(args, &mut i));
            let bkey = key_str(&read_val(args, &mut i));
            let defaults = read_val(args, &mut i);
            let label = read_val(args, &mut i);
            // Nullish target / defaults throws at the proxy construction or
            // the `defaults[rKey]` read; a non-object target survives the
            // reads but throws in the strict-mode write-back — the lifecycle
            // always resets, so every non-Obj target ends in the [1] status.
            if matches!(defaults, JsVal::Null | JsVal::Undef | JsVal::Absent)
                || !matches!(target, JsVal::Obj(_))
            {
                out.push(1.0);
                return out;
            }
            let read = |t: &JsVal, k: &str| -> JsVal {
                match t {
                    JsVal::Obj(fields) => fields
                        .iter()
                        .find(|(kk, _)| kk == k)
                        .map(|(_, v)| v.clone())
                        .unwrap_or(JsVal::Undef),
                    _ => JsVal::Undef,
                }
            };
            let keys = [rkey.as_str(), gkey.as_str(), bkey.as_str()];
            let rgb_default: Vec<JsVal> =
                keys.iter().map(|k| read(&defaults, k)).collect();
            let proxy_draw: Vec<JsVal> = keys.iter().map(|k| read(&target, k)).collect();
            let m = args[i] as usize;
            i += 1;
            let mut muts: Vec<[JsVal; 3]> = Vec::with_capacity(m);
            for _ in 0..m {
                muts.push([read_val(args, &mut i), read_val(args, &mut i), read_val(args, &mut i)]);
            }
            let is_mod = |t: &JsVal| -> bool {
                js_ne(&read(t, keys[0]), &rgb_default[0])
                    || js_ne(&read(t, keys[1]), &rgb_default[1])
                    || js_ne(&read(t, keys[2]), &rgb_default[2])
            };
            out.push(0.0);
            out.push(4.0);
            push_name(&mut out, &label);
            for x in &proxy_draw {
                push_val(&mut out, x);
            }
            out.push(if is_mod(&target) { 1.0 } else { 0.0 });
            for [r, g, b] in &muts {
                let JsVal::Obj(fields) = &mut target else {
                    unreachable!("debug_gui: color lifecycle target");
                };
                map_set(fields, keys[0], r.clone());
                map_set(fields, keys[1], g.clone());
                map_set(fields, keys[2], b.clone());
                out.push(if is_mod(&target) { 1.0 } else { 0.0 });
            }
            push_str(&mut out, &color_hex(&rgb_default));
            // resetToDefault: three writes + proxy.color = defaults.
            let JsVal::Obj(fields) = &mut target else {
                unreachable!("debug_gui: color lifecycle reset target");
            };
            for (j, k) in keys.iter().enumerate() {
                map_set(fields, k, rgb_default[j].clone());
            }
            for x in &rgb_default {
                push_val(&mut out, x);
            }
            push_val(&mut out, &target);
        }
        5 => {
            let mut s = read_val(args, &mut i);
            let d = read_val(args, &mut i);
            let m = args[i] as usize;
            i += 1;
            for _ in 0..m {
                let np = args[i] as usize;
                i += 1;
                let mut path = Vec::with_capacity(np);
                for _ in 0..np {
                    path.push(read_str(args, &mut i));
                }
                let key = read_str(args, &mut i);
                let v = read_val(args, &mut i);
                set_field(&mut s, &path, &key, v);
            }
            let tree = build_tree(&s, &d);
            out.push(tree.len() as f64);
            push_tree(&mut out, &tree, &mut s);
        }
        6 => out.push(LINES_PER_PLAYER),
        k => unreachable!("debug_gui: unknown op kind {k}"),
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn obj(pairs: &[(&str, JsVal)]) -> JsVal {
        JsVal::Obj(pairs.iter().map(|(k, v)| (k.to_string(), v.clone())).collect())
    }

    #[test]
    fn folder_closed_quirks() {
        // Default parameter / ?? fallback / raw survivors.
        assert_eq!(folder_closed(&JsVal::Undef), Some(JsVal::Bool(true)));
        assert_eq!(folder_closed(&JsVal::Absent), Some(JsVal::Bool(true)));
        assert_eq!(folder_closed(&JsVal::Null), None);
        assert_eq!(folder_closed(&obj(&[])), Some(JsVal::Bool(true)));
        assert_eq!(
            folder_closed(&obj(&[("closed", JsVal::Bool(false))])),
            Some(JsVal::Bool(false))
        );
        assert_eq!(
            folder_closed(&obj(&[("closed", JsVal::Num(0.0))])),
            Some(JsVal::Num(0.0))
        );
        assert_eq!(
            folder_closed(&obj(&[("closed", JsVal::Str(String::new()))])),
            Some(JsVal::Str(String::new()))
        );
        assert!(matches!(
            folder_closed(&obj(&[("closed", JsVal::Num(f64::NAN))])),
            Some(JsVal::Num(n)) if n.is_nan()
        ));
        // nullish closed -> true; non-object opts -> wrapper read undefined.
        assert_eq!(
            folder_closed(&obj(&[("closed", JsVal::Null)])),
            Some(JsVal::Bool(true))
        );
        assert_eq!(
            folder_closed(&obj(&[("closed", JsVal::Undef)])),
            Some(JsVal::Bool(true))
        );
        assert_eq!(folder_closed(&JsVal::Num(5.0)), Some(JsVal::Bool(true)));
        assert_eq!(
            folder_closed(&JsVal::Str("x".into())),
            Some(JsVal::Bool(true))
        );
    }

    #[test]
    fn strict_ne_domain() {
        let nan = JsVal::Num(f64::NAN);
        assert!(js_ne(&nan, &nan)); // NaN !== NaN
        assert!(!js_ne(&JsVal::Num(-0.0), &JsVal::Num(0.0))); // -0 === 0
        assert!(!js_ne(&JsVal::Num(0.0), &JsVal::Num(-0.0)));
        assert!(js_ne(&JsVal::Bool(false), &JsVal::Undef));
        assert!(!js_ne(&JsVal::Absent, &JsVal::Undef)); // missing key === undefined
        assert!(js_ne(&JsVal::Null, &JsVal::Undef));
        assert!(js_ne(&JsVal::Num(1.0), &JsVal::Bool(true))); // type-strict
        assert!(js_ne(&JsVal::Str("a".into()), &JsVal::Str("b".into())));
    }

    #[test]
    fn hex_string_table() {
        assert_eq!(to_hex_string(f64::NAN), "NaN");
        assert_eq!(to_hex_string(f64::INFINITY), "Infinity");
        assert_eq!(to_hex_string(f64::NEG_INFINITY), "-Infinity");
        assert_eq!(to_hex_string(-0.0), "0");
        assert_eq!(to_hex_string(0.0), "0");
        assert_eq!(to_hex_string(255.0), "ff");
        assert_eq!(to_hex_string(10.0), "a");
        assert_eq!(to_hex_string(-10.0), "-a");
        assert_eq!(to_hex_string(-127.0), "-7f");
        assert_eq!(to_hex_string(9007199254740992.0), "20000000000000");
        assert_eq!(to_hex_string(9007199254740994.0), "20000000000002");
        // V8: (1e21).toString(16) === "3635c9adc5dea00000" (the double is
        // 1000000000000000000000 = 0x3635C9ADC5DEA00000 exactly).
        assert_eq!(to_hex_string(1e21), "3635c9adc5dea00000");
    }

    #[test]
    fn color_hex_quirks() {
        // Math.round half-up AFTER the *255 multiply.
        let n = |v: f64| JsVal::Num(v);
        assert_eq!(color_hex(&[n(0.5), n(0.5), n(0.5)]), "#808080"); // 127.5 -> 128
        assert_eq!(color_hex(&[n(-0.5), n(-0.5), n(-0.5)]), "#-7f-7f-7f"); // -127.5 -> -127
        assert_eq!(color_hex(&[n(2.5), n(1.0), n(0.0)]), "#27eff00"); // 637.5 -> 638 = 0x27e
        assert_eq!(color_hex(&[n(-0.0), n(0.0), n(1.0)]), "#0000ff");
        assert_eq!(color_hex(&[n(f64::NAN), n(f64::INFINITY), n(1.0)]), "#NaNInfinityff");
        assert_eq!(color_hex(&[JsVal::Undef, JsVal::Null, JsVal::Bool(true)]), "#NaN00ff");
        assert_eq!(color_hex(&[JsVal::Str("0.5".into())]), "#80");
    }

    #[test]
    fn truthiness_gate() {
        assert!(!js_truthy(&JsVal::Absent));
        assert!(!js_truthy(&JsVal::Num(f64::NAN)));
        assert!(!js_truthy(&JsVal::Num(-0.0)));
        assert!(!js_truthy(&JsVal::Str(String::new())));
        assert!(js_truthy(&JsVal::Num(5.0)));
        assert!(js_truthy(&JsVal::Str("x".into())));
    }

    #[test]
    fn lifecycle_reset_appends_missing_key() {
        // defaults missing the key -> undefined; reset writes it (append).
        let res = run_op(
            1,
            &[
                6.0, 0.0, // target {}
                5.0, 1.0, 107.0, // key "k"
                6.0, 0.0, // defaults {}
                1.0, // label undefined
                0.0, // m
            ],
        );
        assert_eq!(res[0], 0.0);
        // im_0: undefined !== undefined -> false
        assert_eq!(res[3], 0.0);
        // After reset the target is {k: undefined} — the missing key APPENDS.
        assert_eq!(&res[4..], &[1.0, 6.0, 1.0, 1.0, 107.0, 1.0]);
    }

    #[test]
    fn lifecycle_nan_modified() {
        let nan = f64::NAN;
        let args = vec![
            6.0, 1.0, 1.0, 118.0, 3.0, nan, // target {v: NaN}
            5.0, 1.0, 118.0, // key "v"
            6.0, 1.0, 1.0, 118.0, 3.0, nan, // defaults {v: NaN}
            1.0, // label undefined
            0.0, // m
        ];
        let res = run_op(1, &args);
        assert_eq!(res[0], 0.0);
        assert_eq!(res[3], 1.0); // NaN !== NaN
    }

    #[test]
    fn build_tree_shape() {
        let s = crate::render_settings::create_render_settings();
        let d = crate::render_settings::create_render_settings();
        let tree = build_tree(&s, &d);
        assert_eq!(tree.len(), 18);
        // Count nodes and confirm the clean tree reports no modifications.
        let mut count = 0usize;
        let mut modified = 0usize;
        let mut s2 = s.clone();
        fn walk(nodes: &[DebugNode], s: &mut JsVal, count: &mut usize, modified: &mut usize) {
            for n in nodes {
                *count += 1;
                match n {
                    DebugNode::Folder { children, .. } => walk(children, s, count, modified),
                    DebugNode::Prop(p) => {
                        if p.is_modified(s) {
                            *modified += 1;
                        }
                        let _ = s;
                    }
                }
            }
        }
        walk(&tree, &mut s2, &mut count, &mut modified);
        assert_eq!(modified, 0);
        assert!(count > 150);
    }

    #[test]
    fn codec_round_trip_kind6() {
        assert_eq!(run_op(6, &[]), vec![2.0]);
    }
}
