//! Port of the runtime subset of `src/core/game/TerrainMapLoader.ts`: the
//! `loadTerrainMap` orchestration (module-level `loadedMaps` cache, the
//! Normal/Compact game-map / mini-map selection, the Compact in-place nation
//! scaling, the Compact spawn-area scaling, the layer placement / alpha
//! validation, and the result construction) plus `genTerrainFromBin`'s
//! buffer-size check and `GameMapImpl` construction. `loadImages` is fixed
//! `false` — the `loadLayerImages` / `createImageBitmap` branch is host-bound
//! and not ported. The manifest / MapData / TerrainMapData interfaces are
//! type-only and erased.
//!
//! Faithfulness notes:
//!
//! * The `loadedMaps` cache is keyed by the template string
//!   `` `${map}:${mapSize}` ``; the port keys on `(map units, size)`, which is
//!   equivalent over the capture's domain (no map name contains a `:`). A hit
//!   returns the cached object without calling `getMapData`; every throw path
//!   (bin length, placement, alpha) leaves the entry uncached, so the same key
//!   retries the full load.
//! * The mini-map branch is a dead ternary: for `Normal` the inner
//!   `mapSize === GameMapSize.Normal ? manifest.map4x : manifest.map16x` is
//!   always true, so the mini map uses `map4x` metadata with the `map4xBin`
//!   data; `Compact` uses `map16x` + `map16xBin`.
//! * Compact scaling rewrites `manifest.nations` / `manifest.additionalNations`
//!   coordinates **in place** (`Math.floor(x / 2)`), so the cached result's
//!   `nations` field aliases the manifest array (JS reference semantics) and a
//!   later Compact load of the *same* manifest object re-scales the already
//!   scaled values — and a cache hit afterwards dumps the mutated array. The
//!   port models this with `Rc<RefCell<Vec<_>>>` shared between the manifest
//!   and every result. `teamGameSpawnAreas` is snapshotted instead: the TS
//!   builds a fresh `scaled` object and nothing mutates the manifest's spawn
//!   areas afterwards.
//! * `Math.max(1, Math.floor(a.width / 2))` keeps JS `Math.max` semantics
//!   (NaN propagates, ±0 rules) via [`crate::game_map::js_max`].
//! * Layer validation runs per layer, placement first then alpha, over the
//!   manifest's live `layers` array. The alpha bound check is JS
//!   `!Number.isFinite(alpha) || alpha < 0 || alpha > 1`: `NaN` fails on the
//!   finite test, `-0` passes (`-0 < 0` is false).
//! * The error messages interpolate JS `Number`→string for the alpha /
//!   width / height / buffer-size slots (`-0` → `"0"`, `NaN` → `"NaN"`,
//!   `Infinity` → `"Infinity"`), shared with `game_ts` via
//!   [`crate::game_ts::js_num_str`]; the map / id / placement slots are plain
//!   strings.
//! * `genTerrainFromBin` checks `data.length !== width * height` *before*
//!   constructing the map, so the ported `GameMap::new` panic path is never
//!   reached from here.
//! * `additionalNations ?? []` keeps the manifest array by reference when the
//!   field is defined (so it shares the in-place scaling), and a fresh empty
//!   array otherwise; the dump's `present` flag records the manifest field
//!   presence, the length always reports the final array.

use std::cell::RefCell;
use std::rc::Rc;

use crate::game_map::{js_max, GameMap};
use crate::game_ts::js_num_str;

/// `MapMetadata` — the `{width, height, num_land_tiles}` triple the manifest
/// carries per resolution.
#[derive(Clone, Copy)]
struct Meta {
    w: f64,
    h: f64,
    nlt: f64,
}

/// `Nation` / `AdditionalNation` as the manifest declares them (plain object
/// literals: `coordinates` optional, `flag` always present as a string in the
/// capture's encoding, `name` required).
#[derive(Clone)]
struct Nation {
    coords: Option<(f64, f64)>,
    flag: Vec<u16>,
    name: Vec<u16>,
}

/// `SpawnArea` — the `{x, y, width, height}` pair inside
/// `TeamGameSpawnAreas`.
#[derive(Clone, Copy)]
struct Area {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

/// `MapLayer` — `{id, placement, alpha?}`.
#[derive(Clone)]
struct Layer {
    id: Vec<u16>,
    placement: Vec<u16>,
    alpha: Option<f64>,
}

/// The scenario's single manifest object. The nations / additionalNations
/// vectors are shared by reference with every cached result (JS aliasing).
struct Manifest {
    map: Meta,
    map4x: Meta,
    map16x: Meta,
    nations: Rc<RefCell<Vec<Nation>>>,
    add_nations: Option<Rc<RefCell<Vec<Nation>>>>,
    tgsa: Option<Vec<(Vec<u16>, Vec<Area>)>>,
    layers: Option<Vec<Layer>>,
}

/// `TerrainMapData` as the cache stores it (`layerImages` is always
/// `undefined` with `loadImages = false` and not dumped).
struct TerrainMapData {
    game_map: GameMap,
    mini_map: GameMap,
    nations: Rc<RefCell<Vec<Nation>>>,
    add_nations: Option<Rc<RefCell<Vec<Nation>>>>,
    tgsa: Option<Vec<(Vec<u16>, Vec<Area>)>>,
    layers: Option<Vec<Layer>>,
}

/// `Math.floor(x / 2)` — JS floor (truncation toward negative infinity,
/// `-0` preserved).
fn floor_half(x: f64) -> f64 {
    (x / 2.0).floor()
}

/// The in-place `nation.coordinates = [floor(x/2), floor(y/2)]` rewrite.
fn scale_nation(n: &mut Nation) {
    if let Some((x, y)) = n.coords {
        n.coords = Some((floor_half(x), floor_half(y)));
    }
}

/// The spawn-area `{x, y, width, height}` → half-size copy with the
/// `Math.max(1, …)` floors.
fn scale_area(a: &Area) -> Area {
    Area {
        x: floor_half(a.x),
        y: floor_half(a.y),
        w: js_max(1.0, floor_half(a.w)),
        h: js_max(1.0, floor_half(a.h)),
    }
}

fn eq_units(u: &[u16], s: &str) -> bool {
    u.len() == s.len() && u.iter().copied().eq(s.encode_utf16())
}

/// `genTerrainFromBin(mapData, data)` — the length check throws the exact TS
/// message (returned as UTF-16 units); a matching buffer constructs the map.
fn gen_terrain(meta: &Meta, data: &[u8]) -> Result<GameMap, Vec<u16>> {
    if data.len() as f64 != meta.w * meta.h {
        let s = format!(
            "Invalid data: buffer size {} incorrect for {}x{} terrain plus 4 bytes for dimensions.",
            data.len(),
            js_num_str(meta.w),
            js_num_str(meta.h),
        );
        return Err(s.encode_utf16().collect());
    }
    Ok(GameMap::new(meta.w, meta.h, data.to_vec(), meta.nlt))
}

// ---------------------------------------------------------------- vectors op
//
// A flat `f64` token runner shared by the golden replay and the wasm probe.
// Strings cross as `[len, u0, .. u(len-1)]` (UTF-16 code units).
//
// kind table (mirrors the capture):
//   0 replay one whole scenario:
//     args [
//       map.w, map.h, map.nlt, map4x.w, .., map16x.w, ..,
//       nNations, (hasCoord, x?, y?, flag, name)*,
//       addPresent, m, (...)*m,
//       tgsaPresent, k, (key, areasLen, (x,y,w,h)*areasLen)*k,
//       layersPresent, l, (id, placement, hasAlpha, alpha?)*l,
//       binKind, len, (byte)*len,        x3  (mapBin, map4xBin, map16xBin;
//                                            kind 1 = zero-length buffer)
//       opsLen, (mapName, size)*opsLen   size 0=Normal, 1=Compact
//     ]
//     res  [opsLen, (per call)*, getMapDataCalls]
//       per call success: [0, gameMap, miniMap, nations, addNations, tgsa,
//                          layers]
//         gameMap / miniMap: [w, h, nlt, terrainLen, (byte)*terrainLen]
//         nations: [n, (hasCoord, x?, y?, flag, name)*n]
//         addNations: [present, m, (...)*m]
//         tgsa: [present, k, (key, areasLen, (x,y,w,h)*areasLen)*k]
//         layers: [present, l, (id, placement, hasAlpha, alpha?)*l]
//       per call throw: [1|2|3, msg]  (bin length / placement / alpha)

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
    fn units(&mut self) -> Vec<u16> {
        let len = self.u();
        (0..len).map(|_| self.f() as u16).collect()
    }
    fn meta(&mut self) -> Meta {
        Meta {
            w: self.f(),
            h: self.f(),
            nlt: self.f(),
        }
    }
    fn nation(&mut self) -> Nation {
        let coords = if self.u() == 1 {
            Some((self.f(), self.f()))
        } else {
            None
        };
        Nation {
            coords,
            flag: self.units(),
            name: self.units(),
        }
    }
    fn bin(&mut self) -> Vec<u8> {
        let kind = self.u();
        let len = self.u();
        let bytes: Vec<u8> = (0..len).map(|_| self.f() as u8).collect();
        if kind == 1 {
            Vec::new()
        } else {
            bytes
        }
    }
}

fn decode_manifest(c: &mut Cur) -> Manifest {
    let map = c.meta();
    let map4x = c.meta();
    let map16x = c.meta();
    let n = c.u();
    let nations = Rc::new(RefCell::new((0..n).map(|_| c.nation()).collect()));
    let add_present = c.u();
    let m = c.u();
    let add_nations = if add_present == 1 {
        Some(Rc::new(RefCell::new((0..m).map(|_| c.nation()).collect())))
    } else {
        None
    };
    let tgsa_present = c.u();
    let k = c.u();
    let tgsa = if tgsa_present == 1 {
        Some(
            (0..k)
                .map(|_| {
                    let key = c.units();
                    let al = c.u();
                    let areas = (0..al)
                        .map(|_| Area {
                            x: c.f(),
                            y: c.f(),
                            w: c.f(),
                            h: c.f(),
                        })
                        .collect();
                    (key, areas)
                })
                .collect(),
        )
    } else {
        None
    };
    let layers_present = c.u();
    let ll = c.u();
    let layers = if layers_present == 1 {
        Some(
            (0..ll)
                .map(|_| {
                    let id = c.units();
                    let placement = c.units();
                    let alpha = if c.u() == 1 { Some(c.f()) } else { None };
                    Layer { id, placement, alpha }
                })
                .collect(),
        )
    } else {
        None
    };
    Manifest {
        map,
        map4x,
        map16x,
        nations,
        add_nations,
        tgsa,
        layers,
    }
}

fn push_units(out: &mut Vec<f64>, u: &[u16]) {
    out.push(u.len() as f64);
    out.extend(u.iter().map(|&x| f64::from(x)));
}

fn push_lit(out: &mut Vec<u16>, s: &str) {
    out.extend(s.encode_utf16());
}

fn push_nation(out: &mut Vec<f64>, n: &Nation) {
    match n.coords {
        Some((x, y)) => {
            out.push(1.0);
            out.push(x);
            out.push(y);
        }
        None => out.push(0.0),
    }
    push_units(out, &n.flag);
    push_units(out, &n.name);
}

fn push_map(out: &mut Vec<f64>, gm: &GameMap) {
    out.push(gm.width());
    out.push(gm.height());
    out.push(gm.num_land_tiles());
    let t = gm.debug_terrain();
    out.push(t.len() as f64);
    for &b in t {
        out.push(f64::from(b));
    }
}

fn push_result(out: &mut Vec<f64>, r: &TerrainMapData) {
    out.push(0.0);
    push_map(out, &r.game_map);
    push_map(out, &r.mini_map);
    let nations = r.nations.borrow();
    out.push(nations.len() as f64);
    for n in nations.iter() {
        push_nation(out, n);
    }
    match &r.add_nations {
        None => {
            out.push(0.0);
            out.push(0.0);
        }
        Some(a) => {
            out.push(1.0);
            let v = a.borrow();
            out.push(v.len() as f64);
            for n in v.iter() {
                push_nation(out, n);
            }
        }
    }
    match &r.tgsa {
        None => out.push(0.0),
        Some(t) => {
            out.push(1.0);
            out.push(t.len() as f64);
            for (key, areas) in t {
                push_units(out, key);
                out.push(areas.len() as f64);
                for a in areas {
                    out.push(a.x);
                    out.push(a.y);
                    out.push(a.w);
                    out.push(a.h);
                }
            }
        }
    }
    match &r.layers {
        None => out.push(0.0),
        Some(l) => {
            out.push(1.0);
            out.push(l.len() as f64);
            for ly in l {
                push_units(out, &ly.id);
                push_units(out, &ly.placement);
                match ly.alpha {
                    None => out.push(0.0),
                    Some(a) => {
                        out.push(1.0);
                        out.push(a);
                    }
                }
            }
        }
    }
}

fn push_err(out: &mut Vec<f64>, kind: f64, msg: &[u16]) {
    out.push(kind);
    push_units(out, msg);
}

/// `` `Map ${map}: layer "${id}" has invalid placement "${placement}" (must
/// be "land" or "water")` ``.
fn placement_msg(map: &[u16], layer: &Layer) -> Vec<u16> {
    let mut m: Vec<u16> = Vec::new();
    push_lit(&mut m, "Map ");
    m.extend(map.iter().copied());
    push_lit(&mut m, ": layer \"");
    m.extend(layer.id.iter().copied());
    push_lit(&mut m, "\" has invalid placement \"");
    m.extend(layer.placement.iter().copied());
    push_lit(&mut m, "\" (must be \"land\" or \"water\")");
    m
}

/// `` `Map ${map}: layer "${id}" has invalid alpha ${alpha} (must be a finite
/// number between 0 and 1)` `` — `${alpha}` is JS `Number`→string.
fn alpha_msg(map: &[u16], layer: &Layer, alpha: f64) -> Vec<u16> {
    let mut m: Vec<u16> = Vec::new();
    push_lit(&mut m, "Map ");
    m.extend(map.iter().copied());
    push_lit(&mut m, ": layer \"");
    m.extend(layer.id.iter().copied());
    push_lit(&mut m, "\" has invalid alpha ");
    m.extend(js_num_str(alpha).encode_utf16());
    push_lit(&mut m, " (must be a finite number between 0 and 1)");
    m
}

pub fn run_op(kind: u8, args: &[f64]) -> Vec<f64> {
    let mut out = Vec::new();
    if kind != 0 {
        return out;
    }
    let mut c = Cur(args, 0);
    let man = decode_manifest(&mut c);
    let bin_map = c.bin();
    let bin_4x = c.bin();
    let bin_16x = c.bin();
    let ops_len = c.u();
    out.push(ops_len as f64);

    // `loadedMaps` — insertion-ordered, keyed by (map units, size flag).
    let mut cache: Vec<(Vec<u16>, usize, TerrainMapData)> = Vec::new();
    let mut calls = 0.0f64;

    for _ in 0..ops_len {
        let map = c.units();
        let size = c.u();
        if let Some(pos) = cache
            .iter()
            .position(|(m, s, _)| *s == size && *m == map)
        {
            push_result(&mut out, &cache[pos].2);
            continue;
        }
        calls += 1.0;
        let normal = size == 0;

        // gameMap: Normal -> map + mapBin, Compact -> map4x + map4xBin.
        let (gm_meta, gm_data) = if normal {
            (man.map, &bin_map)
        } else {
            (man.map4x, &bin_4x)
        };
        let game_map = match gen_terrain(&gm_meta, gm_data) {
            Ok(g) => g,
            Err(msg) => {
                push_err(&mut out, 1.0, &msg);
                continue;
            }
        };
        // miniGameMap: the dead ternary — Normal always takes manifest.map4x
        // with the map4xBin data; Compact takes map16x + map16xBin.
        let (mm_meta, mm_data) = if normal {
            (man.map4x, &bin_4x)
        } else {
            (man.map16x, &bin_16x)
        };
        let mini_map = match gen_terrain(&mm_meta, mm_data) {
            Ok(g) => g,
            Err(msg) => {
                push_err(&mut out, 1.0, &msg);
                continue;
            }
        };

        // Compact: in-place coordinate scaling of the shared manifest arrays.
        if !normal {
            for n in man.nations.borrow_mut().iter_mut() {
                scale_nation(n);
            }
            if let Some(a) = &man.add_nations {
                for n in a.borrow_mut().iter_mut() {
                    scale_nation(n);
                }
            }
        }

        // teamGameSpawnAreas: Compact builds a fresh scaled object (key
        // insertion order preserved); Normal passes the manifest's through.
        let tgsa = match (&man.tgsa, normal) {
            (Some(t), false) => Some(
                t.iter()
                    .map(|(k, areas)| (k.clone(), areas.iter().map(scale_area).collect()))
                    .collect(),
            ),
            (Some(t), true) => Some(t.clone()),
            (None, _) => None,
        };

        // Layer validation: placement first, then alpha, per layer.
        if let Some(layers) = &man.layers {
            let mut err: Option<(f64, Vec<u16>)> = None;
            for layer in layers {
                if !eq_units(&layer.placement, "land") && !eq_units(&layer.placement, "water") {
                    err = Some((2.0, placement_msg(&map, layer)));
                    break;
                }
                if let Some(a) = layer.alpha {
                    if !a.is_finite() || a < 0.0 || a > 1.0 {
                        err = Some((3.0, alpha_msg(&map, layer, a)));
                        break;
                    }
                }
            }
            if let Some((k, msg)) = err {
                push_err(&mut out, k, &msg);
                continue;
            }
        }

        let result = TerrainMapData {
            game_map,
            mini_map,
            nations: Rc::clone(&man.nations),
            add_nations: man.add_nations.as_ref().map(Rc::clone),
            tgsa,
            layers: man.layers.clone(),
        };
        cache.push((map, size, result));
        push_result(&mut out, &cache.last().unwrap().2);
    }
    out.push(calls);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enc_str(s: &str) -> Vec<f64> {
        let u: Vec<u16> = s.encode_utf16().collect();
        let mut v = vec![u.len() as f64];
        v.extend(u.iter().map(|&x| f64::from(x)));
        v
    }

    /// Decode one `[len, u0, ..]` string token starting at `i`, returning the
    /// string and the index just past it.
    fn read_str(res: &[f64], i: usize) -> (String, usize) {
        let len = res[i] as usize;
        let units: Vec<u16> = res[i + 1..i + 1 + len].iter().map(|&u| u as u16).collect();
        (String::from_utf16_lossy(&units), i + 1 + len)
    }

    /// A hand-built scenario. Defaults: map 2x2 (nlt 7), map4x 2x2 (nlt 5),
    /// map16x 2x2 (nlt 3); one nation coords (5,6); additionalNations present
    /// with one no-coord nation; tgsa present with one key "duo" and one area
    /// (3,4,5,6); one land layer with no alpha; all three bins valid 4 bytes.
    struct Spec {
        nations: Vec<f64>,
        add_present: f64,
        add_nations: Vec<f64>,
        tgsa_present: f64,
        tgsa: Vec<f64>,
        layers_present: f64,
        layers: Vec<f64>,
        bin_kind: [f64; 3],
        ops: Vec<(Vec<f64>, f64)>,
    }
    impl Default for Spec {
        fn default() -> Self {
            let mut nations = vec![1.0, 5.0, 6.0]; // hasCoord, x, y
            nations.extend(enc_str("")); // flag
            nations.extend(enc_str("Alpha")); // name
            let mut add = vec![0.0]; // no coords
            add.extend(enc_str(""));
            add.extend(enc_str("Beta"));
            let mut tgsa = enc_str("duo"); // key
            tgsa.extend([1.0, 3.0, 4.0, 5.0, 6.0]); // areasLen + one area
            let mut layers = enc_str("L");
            layers.extend(enc_str("land"));
            layers.push(0.0);
            Self {
                nations,
                add_present: 1.0,
                add_nations: add,
                tgsa_present: 1.0,
                tgsa,
                layers_present: 1.0,
                layers,
                bin_kind: [0.0, 0.0, 0.0],
                ops: vec![(enc_str("m1"), 0.0)],
            }
        }
    }
    impl Spec {
        fn build(&self) -> Vec<f64> {
            let mut a = vec![
                2.0, 2.0, 7.0, // map
                2.0, 2.0, 5.0, // map4x
                2.0, 2.0, 3.0, // map16x
                1.0, // nations: exactly one entry in these tests
            ];
            a.extend(self.nations.iter().copied());
            a.push(self.add_present);
            a.push(if self.add_present == 1.0 { 1.0 } else { 0.0 });
            a.extend(self.add_nations.iter().copied());
            a.push(self.tgsa_present);
            a.push(if self.tgsa_present == 1.0 { 1.0 } else { 0.0 });
            a.extend(self.tgsa.iter().copied());
            a.push(self.layers_present);
            a.push(if self.layers_present == 1.0 { 1.0 } else { 0.0 });
            a.extend(self.layers.iter().copied());
            for b in 0..3 {
                a.push(self.bin_kind[b]);
                a.push(4.0);
                a.extend([1.0, 2.0, 3.0, 4.0]);
            }
            a.push(self.ops.len() as f64);
            for (name, size) in &self.ops {
                a.extend(name.iter().copied());
                a.push(*size);
            }
            a
        }
    }

    #[test]
    fn normal_basic_maps_and_passthrough() {
        let res = run_op(0, &Spec::default().build());
        assert_eq!(res[0], 1.0); // opsLen
        assert_eq!(res[1], 0.0); // success
        assert_eq!(&res[2..6], &[2.0, 2.0, 7.0, 4.0]); // gameMap = map meta
        assert_eq!(&res[10..14], &[2.0, 2.0, 5.0, 4.0]); // mini = map4x meta
        // nations unscaled on Normal: coords stay (5,6).
        assert_eq!(&res[18..22], &[1.0, 1.0, 5.0, 6.0]);
        assert_eq!(*res.last().unwrap(), 1.0); // one getMapData call
    }

    #[test]
    fn compact_scales_in_place_and_spawn_areas() {
        let s = Spec {
            ops: vec![(enc_str("c1"), 1.0)],
            ..Spec::default()
        };
        let res = run_op(0, &s.build());
        assert_eq!(res[4], 5.0); // gameMap uses map4x meta (nlt 5)
        assert_eq!(res[12], 3.0); // mini uses map16x meta (nlt 3)
        // nations scaled in place: floor(5/2)=2, floor(6/2)=3.
        assert_eq!(&res[18..22], &[1.0, 1.0, 2.0, 3.0]);
    }

    #[test]
    fn cache_hit_skips_get_map_data() {
        let s = Spec {
            ops: vec![(enc_str("h1"), 0.0), (enc_str("h1"), 0.0)],
            ..Spec::default()
        };
        let res = run_op(0, &s.build());
        assert_eq!(res[0], 2.0);
        assert_eq!(*res.last().unwrap(), 1.0); // second op hit the cache
    }

    #[test]
    fn compact_then_second_compact_re_scales_polluted_nations() {
        // A Compact load mutates the shared manifest nations; a second load of
        // a DIFFERENT map name re-scales the already-scaled values (5,6 ->
        // 2,3 on op1, 2,3 -> 1,1 on op2).
        let s = Spec {
            ops: vec![(enc_str("k1"), 1.0), (enc_str("k2"), 1.0)],
            ..Spec::default()
        };
        let res = run_op(0, &s.build());
        assert_eq!(&res[18..22], &[1.0, 1.0, 2.0, 3.0]); // op1: scaled once
        assert_eq!(*res.last().unwrap(), 2.0); // two distinct keys, two calls
    }

    #[test]
    fn bin_throw_message_and_no_cache() {
        let s = Spec {
            bin_kind: [1.0, 0.0, 0.0], // mapBin rejected
            ops: vec![(enc_str("b1"), 0.0), (enc_str("b1"), 0.0)],
            ..Spec::default()
        };
        let res = run_op(0, &s.build());
        assert_eq!(res[1], 1.0); // bin throw
        let (msg, _) = read_str(&res, 2);
        assert_eq!(
            msg,
            "Invalid data: buffer size 0 incorrect for 2x2 terrain plus 4 bytes for dimensions."
        );
        assert_eq!(*res.last().unwrap(), 2.0); // throw path never caches
    }

    #[test]
    fn placement_throw_message() {
        let mut layers = enc_str("L");
        layers.extend(enc_str("sky"));
        layers.push(0.0);
        let s = Spec { layers, ..Spec::default() };
        let res = run_op(0, &s.build());
        assert_eq!(res[1], 2.0); // placement throw
        let (msg, _) = read_str(&res, 2);
        assert_eq!(
            msg,
            "Map m1: layer \"L\" has invalid placement \"sky\" (must be \"land\" or \"water\")"
        );
    }

    #[test]
    fn alpha_throw_message_interpolation() {
        let mut layers = enc_str("L");
        layers.extend(enc_str("land"));
        layers.extend([1.0, -0.5]); // hasAlpha, alpha -0.5
        let s = Spec { layers, ..Spec::default() };
        let res = run_op(0, &s.build());
        assert_eq!(res[1], 3.0); // alpha throw
        let (msg, _) = read_str(&res, 2);
        assert_eq!(
            msg,
            "Map m1: layer \"L\" has invalid alpha -0.5 (must be a finite number between 0 and 1)"
        );
    }

    #[test]
    fn alpha_negative_zero_is_legal() {
        let mut layers = enc_str("L");
        layers.extend(enc_str("land"));
        layers.extend([1.0, -0.0]); // hasAlpha, alpha -0
        let s = Spec { layers, ..Spec::default() };
        let res = run_op(0, &s.build());
        assert_eq!(res[1], 0.0); // success: -0 passes the bounds test
    }

    #[test]
    fn additional_nations_absent_dumps_present_zero() {
        let s = Spec {
            add_present: 0.0,
            add_nations: vec![],
            ..Spec::default()
        };
        let res = run_op(0, &s.build());
        assert_eq!(res[1], 0.0); // success
        // nations dump spans res[18..29]; additionalNations ?? [] -> [0, 0].
        assert_eq!(&res[29..31], &[0.0, 0.0]);
    }

    #[test]
    fn js_floor_half_and_max_edges() {
        assert_eq!(floor_half(-3.0), -2.0);
        assert_eq!(floor_half(1.0), 0.0);
        assert!(floor_half(-0.0).is_sign_negative());
        assert_eq!(js_max(1.0, floor_half(1.0)), 1.0); // max(1, 0)
        assert!(js_max(1.0, f64::NAN).is_nan());
    }
}

