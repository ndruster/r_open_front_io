// One-shot generator for crates/core/src/maps_gen.rs.
//
// Reads the real `src/core/game/Maps.gen.ts` through the strip-mode shim and
// rewrites the region between the BEGIN/END GENERATED markers with Rust data
// tables (GAME_MAP_TYPES, MAP_CATEGORY_ORDER, MAPS). Rerun after an upstream
// Maps.gen.ts change; the golden vectors (gen_vectors.mjs) then re-pin the
// serialised dump.
import { readFileSync, writeFileSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { loadTs, TS_ROOT } from "./ts_load.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "..");

const MG = await loadTs("src/core/game/Maps.gen.ts");

const rustStr = (s) => {
  let out = "";
  for (const ch of s) {
    if (ch === "\\") out += "\\\\";
    else if (ch === '"') out += '\\"';
    else if (ch === "\n") out += "\\n";
    else if (ch === "\r") out += "\\r";
    else if (ch === "\t") out += "\\t";
    else if (ch.codePointAt(0) < 0x20)
      out += `\\u{${ch.codePointAt(0).toString(16)}}`;
    else out += ch;
  }
  return `"${out}"`;
};

const rustNum = (v) => {
  if (typeof v !== "number" || Number.isNaN(v))
    throw new Error(`non-numeric value ${v}`);
  if (Object.is(v, -0)) return "-0.0";
  if (!Number.isFinite(v))
    throw new Error(`non-finite value ${v}`);
  if (v === Math.trunc(v) && Math.abs(v) < 1e15) return `${v}.0`;
  throw new Error(`non-integer value ${v} - extend the generator`);
};

const strList = (arr) =>
  arr.length === 0 ? "&[]" : `&[${arr.map(rustStr).join(", ")}]`;

// --- GameMapType --------------------------------------------------------------
const enumEntries = Object.entries(MG.GameMapType);
const L = [];
L.push("// BEGIN GENERATED — rewritten by tools/gen_maps_rs.mjs, do not edit by hand.");
L.push("/// `GameMapType`: the 127 enum members in declaration order.");
L.push("pub static GAME_MAP_TYPES: &[MapTypeEntry] = &[");
for (const [name, value] of enumEntries)
  L.push(`    MapTypeEntry { name: ${rustStr(name)}, value: ${rustStr(value)} },`);
L.push("];");
L.push("");

// --- mapCategoryOrder ----------------------------------------------------------
L.push("/// `mapCategoryOrder`: the 16 categories in picker display order.");
L.push("pub static MAP_CATEGORY_ORDER: &[&str] = &[");
for (const c of MG.mapCategoryOrder) L.push(`    ${rustStr(c)},`);
L.push("];");
L.push("");

// --- maps ----------------------------------------------------------------------
L.push("/// `maps`: the 127 `MapInfo` records in source order.");
L.push("pub static MAPS: &[MapInfo] = &[");
for (const m of MG.maps) {
  L.push("    MapInfo {");
  L.push(`        id: ${rustStr(m.id)},`);
  L.push(`        type_: ${rustStr(m.type)},`);
  L.push(`        translation_key: ${rustStr(m.translationKey)},`);
  L.push(`        categories: ${strList(m.categories)},`);
  L.push(`        multiplayer_frequency: ${rustNum(m.multiplayerFrequency)},`);
  L.push(`        ffa_frequency: ${rustNum(m.ffaFrequency)},`);
  L.push(`        team_frequency: ${rustNum(m.teamFrequency)},`);
  L.push(`        special_frequency: ${rustNum(m.specialFrequency)},`);
  L.push(`        default_nation_count: ${rustNum(m.defaultNationCount)},`);
  L.push(
    `        featured_rank: ${m.featuredRank === undefined ? "None" : `Some(${rustNum(m.featuredRank)})`},`,
  );
  L.push(
    `        special_team_count: ${m.specialTeamCount === undefined ? "None" : `Some(${rustNum(m.specialTeamCount)})`},`,
  );
  L.push(
    `        disabled_modifiers: ${m.disabledModifiers === undefined ? "None" : `Some(${strList(m.disabledModifiers)})`},`,
  );
  L.push(
    `        forced_modifiers: ${m.forcedModifiers === undefined ? "None" : `Some(${strList(m.forcedModifiers)})`},`,
  );
  L.push(
    `        themes: ${m.themes === undefined ? "None" : `Some(${strList(m.themes)})`},`,
  );
  if (m.customTribes === undefined) {
    L.push("        custom_tribes: None,");
  } else {
    L.push(`        custom_tribes: Some(&[`);
    for (const t of m.customTribes) {
      const coords =
        t.coordinates === undefined
          ? "None"
          : `Some((${rustNum(t.coordinates[0])}, ${rustNum(t.coordinates[1])}))`;
      L.push(`            Tribe { name: ${rustStr(t.name)}, coordinates: ${coords} },`);
    }
    L.push("        ]),");
  }
  if (m.layers === undefined) {
    L.push("        layers: None,");
  } else {
    L.push(`        layers: Some(&[`);
    for (const l of m.layers) {
      const nuke =
        l.nukeable === undefined ? "None" : `Some(${l.nukeable ? "true" : "false"})`;
      L.push(
        `            Layer { id: ${rustStr(l.id)}, placement: ${rustStr(l.placement)}, nukeable: ${nuke} },`,
      );
    }
    L.push("        ]),");
  }
  L.push("    },");
}
L.push("];");
L.push("// END GENERATED");

const rsPath = join(root, "crates", "core", "src", "maps_gen.rs");
const src = readFileSync(rsPath, "utf8");
const begin = src.indexOf("// BEGIN GENERATED");
const end = src.indexOf("// END GENERATED");
if (begin < 0 || end < 0)
  throw new Error("maps_gen.rs: GENERATED markers not found");
const out =
  src.slice(0, begin) + L.join("\n") + src.slice(end + "// END GENERATED".length);
writeFileSync(rsPath, out, "utf8");
console.log(
  `maps_gen.rs: ${enumEntries.length} types, ${MG.mapCategoryOrder.length} categories, ${MG.maps.length} maps`,
);
