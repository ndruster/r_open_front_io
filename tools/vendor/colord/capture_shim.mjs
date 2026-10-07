// S15 d capture facade over the vendored REAL colord 2.9.3 (index.mjs).
// ColorAllocator / ThemeProvider import "colord" through this shim in the
// prepared TS copies: every colord(input) construction is memoized by a
// deterministic key string and assigned an auto-increment id, and the Colord
// prototype is monkey-patched so every observation the real package produces
// (toRgb / toLab / toLch / toHsl / toRgbString / darken / alpha / delta) is
// recorded into globalThis.__COLORD_TRACE. The capture resets the trace per
// op and ships the full table with the golden; the Rust twin replays the
// control flow over the ids, so the genuine parser / CIEDE2000 / rounding
// quirks stay pinned by V8 without the port reimplementing colord.
//
// Memoization contract: colord(key) and darken/alpha(id, amount) return the
// SAME instance on repeated calls, and each observation row is recorded
// ONCE per (id) (per (id1,id2) for delta) — the results are pure, so
// duplicates carry no information. The Rust table lookup takes the first
// entry; a miss is a capture bug and panics there.
import { Colord, colord as realColord, extend as realExtend } from "./index.mjs";
import labPlugin from "./plugins/lab.mjs";
import lchPlugin from "./plugins/lch.mjs";

export { labPlugin, lchPlugin };

const emptyTrace = () => ({
  construct: [],
  toRgb: [],
  toLab: [],
  toLch: [],
  toHsl: [],
  toRgbString: [],
  darken: [],
  alpha: [],
  delta: [],
});

export const TRACE = (globalThis.__COLORD_TRACE ||= emptyTrace());

// Deterministic input key string (NOT JSON.stringify: it collapses -0 to 0
// and NaN to null). Mirrored byte-for-byte by `keystring` in the Rust twin:
//   string  -> "S<len>:<u0,u1,..>"   (UTF-16 units)
//   number  -> "N<js String(v)>"
//   object  -> "O<k>=<String(v)>;..;" (insertion order, number fields)
const encS = (s) =>
  `S${s.length}:${Array.from({ length: s.length }, (_, i) => s.charCodeAt(i)).join(",")}`;
const keystring = (input) => {
  if (typeof input === "string") return encS(input);
  if (typeof input === "number") return `N${String(input)}`;
  return `O${Object.entries(input)
    .map(([k, v]) => `${k}=${String(v)}`)
    .join(";")};`;
};

// [len, u0, ..] token stream for a key string (the Rust read_str form).
const encSrow = (s) => [s.length, ...Array.from({ length: s.length }, (_, i) => s.charCodeAt(i))];

let idOf = new WeakMap();
let byKey = new Map();
let byId = new Map();
let derivedBy = new Map();
let seenObs = new Map(); // name -> Set of dedup keys
let nextId = 1;

export { byId };

export function resetTrace() {
  Object.assign(TRACE, emptyTrace());
  idOf = new WeakMap();
  byKey = new Map();
  byId = new Map();
  derivedBy = new Map();
  seenObs = new Map();
  nextId = 1;
}

function ensureId(inst) {
  let id = idOf.get(inst);
  if (id === undefined) {
    id = nextId++;
    idOf.set(inst, id);
    byId.set(id, inst); // darken/alpha-derived instances join the getter sweep
  }
  return id;
}

function recordOnce(name, key, row) {
  let seen = seenObs.get(name);
  if (seen === undefined) {
    seen = new Set();
    seenObs.set(name, seen);
  }
  if (!seen.has(key)) {
    seen.add(key);
    TRACE[name].push(row);
  }
}

export function colord(input) {
  if (input instanceof Colord) return input; // w() passthrough: same instance
  const ks = keystring(input);
  let hit = byKey.get(ks);
  if (hit === undefined) {
    const inst = realColord(input);
    const id = nextId++;
    idOf.set(inst, id);
    hit = { id, inst };
    byKey.set(ks, hit);
    byId.set(id, inst);
    TRACE.construct.push([...encSrow(ks), id]);
  }
  return hit.inst;
}

// Field order of each observation object, matching the colord return
// literals (toLab yields {l,a,b,alpha}, toLch {l,c,h,a}, toHsl {h,s,l,a}).
const ORDER = {
  toRgb: ["r", "g", "b", "a"],
  toLab: ["l", "a", "b", "alpha"],
  toLch: ["l", "c", "h", "a"],
  toHsl: ["h", "s", "l", "a"],
};

function patch(name) {
  const impl = Colord.prototype[name];
  // toLab / toLch / delta only exist after extend([lab/lch]) installs them;
  // extend() re-runs patchAll, so the wrappers land on the plugin versions.
  if (impl === undefined || impl.__colordTraceBase !== undefined) return;
  const w = function (...args) {
    const id = ensureId(this);
    if (name === "delta") {
      const other = args[0] instanceof Colord ? args[0] : colord(args[0]);
      const oid = ensureId(other);
      const d = impl.call(this, other);
      recordOnce("delta", `${id}\0${oid}`, [id, oid, d]);
      return d;
    }
    if (name === "darken" || name === "alpha") {
      if (args.length === 0) return impl.call(this); // getter form: not used
      const mk = `${name}\0${id}\0${String(args[0])}`;
      let hit = derivedBy.get(mk);
      if (hit === undefined) {
        const inst = impl.call(this, args[0]);
        const rid = ensureId(inst);
        hit = { id: rid, inst };
        derivedBy.set(mk, hit);
        TRACE[name].push([id, args[0], rid]);
      }
      return hit.inst;
    }
    const r = impl.call(this);
    if (name === "toRgbString") {
      recordOnce("toRgbString", id, [id, ...encSrow(r)]);
    } else {
      recordOnce(name, id, [id, ...ORDER[name].map((k) => r[k])]);
    }
    return r;
  };
  w.__colordTraceBase = impl;
  Colord.prototype[name] = w;
}

function patchAll() {
  for (const n of Object.keys(ORDER)) patch(n);
  patch("toRgbString");
  patch("darken");
  patch("alpha");
  patch("delta");
}

export function extend(plugins) {
  realExtend(plugins);
  patchAll();
}

export { Colord };

patchAll();
