//! Port of the pure logic of `src/client/hud/Tutorial.ts` — the `TutorialContext`
//! snapshot, the 22-step `TUTORIAL_STEPS` predicate table and the
//! `TutorialProgress` cursor state machine (`STEP_DONE_LINGER_TICKS = 15`).
//! `TutorialHighlightEvent` (the `GameEvent` bus wrapper) is out of scope.
//!
//! Faithfulness notes (quirk list):
//!
//! * `applies` / `isDone` are optional closures; the port models them as
//!   `Option<fn(&TutorialContext) -> bool>` tables written closure-for-closure
//!   against the TS source. `stepApplies` is `steps[i].applies?.(ctx) ?? true`
//!   — a missing `applies` applies. `current()` past the end reads `null`
//!   (`steps[index] ?? null`), so `finished()` and the `?.` chains agree.
//! * `acknowledge` gates `step?.manual && doneTicks === null` — `manual` is
//!   the literal `true` in TS, so a truthy check equals a `Some(true)` check;
//!   an already-done step is inert.
//! * `update` order is load-bearing: the `countCtx` latch takes the FIRST
//!   context whose `hasSpawned` is true (strict, no nullish) and never
//!   re-snapshots; `doneTicks++` happens BEFORE the `< 15` return (so the
//!   counter is visible as 1..14 while lingering); the skip `while` loop and
//!   the `step?.isDone?.(ctx)` re-arm run after the advance.
//! * `position` / `total` count over `countCtx ?? ctx` — the latched snapshot
//!   when it exists, the live context before spawn. `applicable` counts the
//!   steps that apply STRICTLY BEFORE the given index (`i < before`), so
//!   `position` is 1-based among applicable steps and `total` counts all.
//! * `gold` / `cityCost` cross the wire as f64 (the ≤ 2^53 domain, same
//!   precedent as `game_info_ranking` bigints); `cityCost !== null` is a
//!   presence gate, `gold >= cityCost` is JS `>=` over numbers.
//! * The step table dumps `id` / `highlight` / `manual` / `bullets` / `unit`
//!   / `hotkey`; `applies` / `isDone` ride as Rust closures. `unit` dumps the
//!   `UnitType` enum VALUE (`"Defense Post"`, `"Missile Silo"`, `"Atom Bomb"`
//!   differ from the member names).

/// `TutorialContext` — the per-tick snapshot the predicates read.
#[derive(Debug, Clone, Default)]
pub struct TutorialContext {
    pub has_spawned: bool,
    pub in_spawn_phase: bool,
    pub attacking: bool,
    pub attack_ratio_moved: bool,
    pub boats_disabled: bool,
    pub boat_sent: bool,
    pub bots_exist: bool,
    pub nations_exist: bool,
    pub alliances_disabled: bool,
    pub allied: bool,
    pub gold: f64,
    /// `null` until the worker reports it.
    pub city_cost: Option<f64>,
    pub city_disabled: bool,
    pub cities: f64,
    pub port_disabled: bool,
    pub ports: f64,
    pub defense_post_disabled: bool,
    pub defense_posts: f64,
    pub factory_disabled: bool,
    pub factories: f64,
    pub warship_disabled: bool,
    pub warships: f64,
    pub silo_disabled: bool,
    pub silos: f64,
    pub atom_disabled: bool,
    pub silo_ready: bool,
    pub atom_launched: bool,
    pub hydrogen_disabled: bool,
    pub mirv_disabled: bool,
    pub sam_disabled: bool,
}

/// The `unit` field of a step — the `UnitType` string-enum VALUE (the TS
/// values differ from the member names: `"Defense Post"`, `"Missile Silo"`,
/// `"Atom Bomb"`; the capture inlines `UnitType` as a plain object).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnitTypeName {
    City,
    Factory,
    Port,
    DefensePost,
    Warship,
    MissileSilo,
    AtomBomb,
}

impl UnitTypeName {
    /// The enum VALUE (what `step.unit` holds in TS).
    pub fn as_str(self) -> &'static str {
        match self {
            UnitTypeName::City => "City",
            UnitTypeName::Factory => "Factory",
            UnitTypeName::Port => "Port",
            UnitTypeName::DefensePost => "Defense Post",
            UnitTypeName::Warship => "Warship",
            UnitTypeName::MissileSilo => "Missile Silo",
            UnitTypeName::AtomBomb => "Atom Bomb",
        }
    }
}

/// `TutorialStep` — the static half (the closures are the table entries).
#[derive(Debug, Clone, Copy)]
pub struct TutorialStep {
    pub id: &'static str,
    pub highlight: Option<&'static str>,
    pub unit: Option<UnitTypeName>,
    pub hotkey: Option<&'static str>,
    pub bullets: Option<&'static [&'static str]>,
    pub manual: bool,
    pub applies: Option<fn(&TutorialContext) -> bool>,
    pub is_done: Option<fn(&TutorialContext) -> bool>,
}

/// `STEP_DONE_LINGER_TICKS`.
pub const STEP_DONE_LINGER_TICKS: f64 = 15.0;

/// `TUTORIAL_STEPS` — the 22 entries in TS declaration order, every
/// `applies` / `isDone` closure transcribed verbatim.
pub const TUTORIAL_STEPS: [TutorialStep; 22] = [
    // spawn: waits out the multiplayer spawn timer too.
    TutorialStep {
        id: "spawn",
        highlight: None,
        unit: None,
        hotkey: None,
        bullets: None,
        manual: false,
        applies: None,
        is_done: Some(|c| c.has_spawned && !c.in_spawn_phase),
    },
    TutorialStep {
        id: "attack_wilderness",
        highlight: Some("territory"),
        unit: None,
        hotkey: None,
        bullets: None,
        manual: false,
        applies: None,
        is_done: Some(|c| c.attacking),
    },
    TutorialStep {
        id: "troops",
        highlight: Some("troops"),
        unit: None,
        hotkey: None,
        bullets: None,
        manual: true,
        applies: None,
        is_done: None,
    },
    TutorialStep {
        id: "troop_rate",
        highlight: Some("troop_rate"),
        unit: None,
        hotkey: None,
        bullets: None,
        manual: true,
        applies: None,
        is_done: None,
    },
    TutorialStep {
        id: "attack_ratio",
        highlight: Some("attack_ratio"),
        unit: None,
        hotkey: None,
        bullets: None,
        manual: false,
        applies: None,
        is_done: Some(|c| c.attack_ratio_moved),
    },
    TutorialStep {
        id: "capture_tribes",
        highlight: Some("tribes"),
        unit: None,
        hotkey: None,
        bullets: None,
        manual: false,
        applies: Some(|c| c.bots_exist && !c.city_disabled),
        is_done: Some(|c| c.cities > 0.0 || matches!(c.city_cost, Some(cc) if c.gold >= cc)),
    },
    TutorialStep {
        id: "buy_city",
        highlight: Some("city"),
        unit: Some(UnitTypeName::City),
        hotkey: Some("buildCity"),
        bullets: None,
        manual: false,
        applies: Some(|c| !c.city_disabled),
        is_done: Some(|c| c.cities > 0.0),
    },
    TutorialStep {
        id: "propose_alliance",
        highlight: Some("nation"),
        unit: None,
        hotkey: None,
        bullets: None,
        manual: false,
        applies: Some(|c| c.nations_exist && !c.alliances_disabled),
        is_done: Some(|c| c.allied),
    },
    TutorialStep {
        id: "alliance_info",
        highlight: None,
        unit: None,
        hotkey: None,
        bullets: Some(&["alliance_info", "traitor_info"]),
        manual: true,
        applies: Some(|c| c.nations_exist && !c.alliances_disabled),
        is_done: None,
    },
    TutorialStep {
        id: "buy_factory",
        highlight: Some("factory"),
        unit: Some(UnitTypeName::Factory),
        hotkey: Some("buildFactory"),
        bullets: None,
        manual: false,
        applies: Some(|c| !c.factory_disabled),
        is_done: Some(|c| c.factories > 0.0),
    },
    TutorialStep {
        id: "factory_info",
        highlight: None,
        unit: None,
        hotkey: None,
        bullets: None,
        manual: true,
        applies: Some(|c| !c.factory_disabled),
        is_done: None,
    },
    TutorialStep {
        id: "send_boat",
        highlight: None,
        unit: None,
        hotkey: None,
        bullets: None,
        manual: false,
        applies: Some(|c| !c.boats_disabled),
        is_done: Some(|c| c.boat_sent),
    },
    TutorialStep {
        id: "buy_port",
        highlight: Some("port"),
        unit: Some(UnitTypeName::Port),
        hotkey: Some("buildPort"),
        bullets: None,
        manual: false,
        applies: Some(|c| !c.port_disabled),
        is_done: Some(|c| c.ports > 0.0),
    },
    TutorialStep {
        id: "port_info",
        highlight: None,
        unit: None,
        hotkey: None,
        bullets: Some(&["port_info_ships", "port_info_warships"]),
        manual: true,
        applies: Some(|c| !c.port_disabled),
        is_done: None,
    },
    TutorialStep {
        id: "buy_defense_post",
        highlight: Some("defense_post"),
        unit: Some(UnitTypeName::DefensePost),
        hotkey: Some("buildDefensePost"),
        bullets: None,
        manual: false,
        applies: Some(|c| !c.defense_post_disabled),
        is_done: Some(|c| c.defense_posts > 0.0),
    },
    TutorialStep {
        id: "buy_warship",
        highlight: Some("warship"),
        unit: Some(UnitTypeName::Warship),
        hotkey: Some("buildWarship"),
        bullets: None,
        manual: false,
        applies: Some(|c| !c.warship_disabled && !c.port_disabled),
        is_done: Some(|c| c.warships > 0.0),
    },
    TutorialStep {
        id: "buy_silo",
        highlight: Some("silo"),
        unit: Some(UnitTypeName::MissileSilo),
        hotkey: Some("buildMissileSilo"),
        bullets: None,
        manual: false,
        applies: Some(|c| !c.silo_disabled),
        is_done: Some(|c| c.silos > 0.0),
    },
    TutorialStep {
        id: "launch_atom",
        highlight: Some("atom"),
        unit: Some(UnitTypeName::AtomBomb),
        hotkey: Some("buildAtomBomb"),
        bullets: None,
        manual: false,
        applies: Some(|c| !c.silo_disabled && !c.atom_disabled),
        is_done: Some(|c| c.atom_launched),
    },
    TutorialStep {
        id: "atom_info",
        highlight: Some("atom"),
        unit: None,
        hotkey: None,
        bullets: None,
        manual: true,
        applies: Some(|c| !c.silo_disabled && !c.atom_disabled),
        is_done: None,
    },
    TutorialStep {
        id: "hydrogen_info",
        highlight: Some("hydrogen"),
        unit: None,
        hotkey: None,
        bullets: None,
        manual: true,
        applies: Some(|c| !c.silo_disabled && !c.hydrogen_disabled),
        is_done: None,
    },
    TutorialStep {
        id: "mirv_info",
        highlight: Some("mirv"),
        unit: None,
        hotkey: None,
        bullets: None,
        manual: true,
        applies: Some(|c| !c.silo_disabled && !c.mirv_disabled),
        is_done: None,
    },
    TutorialStep {
        id: "sam_info",
        highlight: Some("sam"),
        unit: None,
        hotkey: None,
        bullets: None,
        manual: true,
        applies: Some(|c| !c.sam_disabled),
        is_done: None,
    },
];

/// `TutorialProgress` — the cursor over the step table.
#[derive(Debug)]
pub struct TutorialProgress {
    index: usize,
    /// `null` while the current step is pending; TS `number | null`.
    done_ticks: Option<f64>,
    /// The first post-spawn context snapshot (by value — the TS object is
    /// never mutated by the ported code, only retained).
    count_ctx: Option<TutorialContext>,
}

impl Default for TutorialProgress {
    fn default() -> Self {
        Self::new()
    }
}

impl TutorialProgress {
    /// `new TutorialProgress()` (the default `TUTORIAL_STEPS` table).
    pub fn new() -> Self {
        Self { index: 0, done_ticks: None, count_ctx: None }
    }

    /// `current()` — `steps[index] ?? null`.
    pub fn current(&self) -> Option<&'static TutorialStep> {
        TUTORIAL_STEPS.get(self.index)
    }

    /// `finished()`.
    pub fn finished(&self) -> bool {
        self.index >= TUTORIAL_STEPS.len()
    }

    /// `stepDone()`.
    pub fn step_done(&self) -> bool {
        self.done_ticks.is_some()
    }

    /// `position(ctx)` — 1-based among applicable steps over the latched
    /// (or live) context.
    pub fn position(&self, ctx: &TutorialContext) -> f64 {
        let c = self.count_ctx.as_ref().unwrap_or(ctx);
        (self.applicable(c, self.index) as f64) + 1.0
    }

    /// `total(ctx)`.
    pub fn total(&self, ctx: &TutorialContext) -> f64 {
        let c = self.count_ctx.as_ref().unwrap_or(ctx);
        self.applicable(c, TUTORIAL_STEPS.len()) as f64
    }

    /// `acknowledge()` — completes a pending manual step
    /// (`step?.manual && this.doneTicks === null`).
    pub fn acknowledge(&mut self) {
        if let Some(step) = self.current() {
            if step.manual && self.done_ticks.is_none() {
                self.done_ticks = Some(0.0);
            }
        }
    }

    /// `skip()` — moves past the current step without completing it.
    pub fn skip(&mut self) {
        if self.finished() {
            return;
        }
        self.index += 1;
        self.done_ticks = None;
    }

    /// `update(ctx)` — one tick.
    pub fn update(&mut self, ctx: &TutorialContext) {
        if self.count_ctx.is_none() && ctx.has_spawned {
            self.count_ctx = Some(ctx.clone());
        }
        if let Some(t) = self.done_ticks {
            let t = t + 1.0; // `this.doneTicks++` — the increment is visible.
            self.done_ticks = Some(t);
            if t < STEP_DONE_LINGER_TICKS {
                return;
            }
            self.index += 1;
            self.done_ticks = None;
        }
        while !self.finished() && !self.step_applies(self.index, ctx) {
            self.index += 1;
        }
        if let Some(step) = self.current() {
            if let Some(f) = step.is_done {
                if f(ctx) {
                    self.done_ticks = Some(0.0);
                }
            }
        }
    }

    fn step_applies(&self, i: usize, ctx: &TutorialContext) -> bool {
        match TUTORIAL_STEPS.get(i).and_then(|s| s.applies) {
            Some(f) => f(ctx),
            None => true,
        }
    }

    fn applicable(&self, ctx: &TutorialContext, before: usize) -> usize {
        let mut n = 0;
        for i in 0..before {
            if self.step_applies(i, ctx) {
                n += 1;
            }
        }
        n
    }
}

// ---------------------------------------------------------------- vectors op
//
// Context wire form (flat f64 tokens, 30 fields in TS interface order):
// 22 booleans (0|1), gold f64 (the <= 2^53 bigint domain), cityCost tag
// (0=null, 1=value f64), then the six numeric counts (cities, ports,
// defensePosts, factories, warships, silos).
//   order: hasSpawned inSpawnPhase attacking attackRatioMoved boatsDisabled
//          boatSent botsExist nationsExist alliancesDisabled allied
//          gold cityCostTag[?] cityDisabled cities portDisabled ports
//          defensePostDisabled defensePosts factoryDisabled factories
//          warshipDisabled warships siloDisabled silos atomDisabled
//          siloReady atomLaunched hydrogenDisabled mirvDisabled samDisabled
//
// kind table:
//   0 [n, (scriptOp, ctx)*n] -> [n, (dump)*n]   one fresh TutorialProgress,
//      then the scripted op chain (scriptOp 1 update(ctx), 2 acknowledge(),
//      3 skip()). Each op dumps the OBSERVABLE state after it runs (the TS
//      capture cannot read the private index / doneTicks / countCtx, so the
//      dump pins current id / finished / stepDone / position(ctx) /
//      total(ctx) — the op chain pins the hidden trajectory):
//        [currentTag 0|1 + id-str, finished, stepDone, position, total]
//   5 [] -> [22, (id, highlightTag[?] + str, manual, unitTag[?] + str,
//              hotkeyTag[?] + str, bulletsTag n [str]*n)*22, 15]
//      step-table dump (unit carries the UnitType VALUE).
//   (the capture always passes FRESH context literals per update — the TS
//    countCtx stores the object by reference, so a reused-and-mutated
//    literal would diverge; the Rust latch clones by value, agreeing with
//    fresh literals).

fn read_ctx(tokens: &[f64], i: &mut usize) -> TutorialContext {
    let b = |tokens: &[f64], i: &mut usize| {
        let v = tokens[*i] != 0.0;
        *i += 1;
        v
    };
    let n = |tokens: &[f64], i: &mut usize| {
        let v = tokens[*i];
        *i += 1;
        v
    };
    TutorialContext {
        has_spawned: b(tokens, i),
        in_spawn_phase: b(tokens, i),
        attacking: b(tokens, i),
        attack_ratio_moved: b(tokens, i),
        boats_disabled: b(tokens, i),
        boat_sent: b(tokens, i),
        bots_exist: b(tokens, i),
        nations_exist: b(tokens, i),
        alliances_disabled: b(tokens, i),
        allied: b(tokens, i),
        gold: n(tokens, i),
        city_cost: if n(tokens, i) == 0.0 { None } else { Some(n(tokens, i)) },
        city_disabled: b(tokens, i),
        cities: n(tokens, i),
        port_disabled: b(tokens, i),
        ports: n(tokens, i),
        defense_post_disabled: b(tokens, i),
        defense_posts: n(tokens, i),
        factory_disabled: b(tokens, i),
        factories: n(tokens, i),
        warship_disabled: b(tokens, i),
        warships: n(tokens, i),
        silo_disabled: b(tokens, i),
        silos: n(tokens, i),
        atom_disabled: b(tokens, i),
        silo_ready: b(tokens, i),
        atom_launched: b(tokens, i),
        hydrogen_disabled: b(tokens, i),
        mirv_disabled: b(tokens, i),
        sam_disabled: b(tokens, i),
    }
}

#[cfg(test)]
fn push_ctx(out: &mut Vec<f64>, c: &TutorialContext) {
    let b = |out: &mut Vec<f64>, v: bool| out.push(if v { 1.0 } else { 0.0 });
    b(out, c.has_spawned);
    b(out, c.in_spawn_phase);
    b(out, c.attacking);
    b(out, c.attack_ratio_moved);
    b(out, c.boats_disabled);
    b(out, c.boat_sent);
    b(out, c.bots_exist);
    b(out, c.nations_exist);
    b(out, c.alliances_disabled);
    b(out, c.allied);
    out.push(c.gold);
    match c.city_cost {
        Some(v) => {
            out.push(1.0);
            out.push(v);
        }
        None => out.push(0.0),
    }
    b(out, c.city_disabled);
    out.push(c.cities);
    b(out, c.port_disabled);
    out.push(c.ports);
    b(out, c.defense_post_disabled);
    out.push(c.defense_posts);
    b(out, c.factory_disabled);
    out.push(c.factories);
    b(out, c.warship_disabled);
    out.push(c.warships);
    b(out, c.silo_disabled);
    out.push(c.silos);
    b(out, c.atom_disabled);
    b(out, c.silo_ready);
    b(out, c.atom_launched);
    b(out, c.hydrogen_disabled);
    b(out, c.mirv_disabled);
    b(out, c.sam_disabled);
}

fn push_dump(out: &mut Vec<f64>, p: &TutorialProgress, ctx: &TutorialContext) {
    // Observable-only: the TS capture cannot read the private index /
    // doneTicks / countCtx, so the dump pins the public surface (current
    // step id, finished, stepDone, position, total) after every scripted
    // op — the op chain itself pins the hidden trajectory.
    match p.current() {
        Some(s) => {
            out.push(1.0);
            crate::js_json::push_str(out, s.id);
        }
        None => out.push(0.0),
    }
    out.push(if p.finished() { 1.0 } else { 0.0 });
    out.push(if p.step_done() { 1.0 } else { 0.0 });
    out.push(p.position(ctx));
    out.push(p.total(ctx));
}

pub fn run_op(kind: u8, args: &[f64]) -> Vec<f64> {
    let mut out = Vec::new();
    match kind {
        5 => {
            out.push(TUTORIAL_STEPS.len() as f64);
            for s in &TUTORIAL_STEPS {
                crate::js_json::push_str(&mut out, s.id);
                match s.highlight {
                    Some(h) => {
                        out.push(1.0);
                        crate::js_json::push_str(&mut out, h);
                    }
                    None => out.push(0.0),
                }
                out.push(if s.manual { 1.0 } else { 0.0 });
                match s.unit {
                    Some(u) => {
                        out.push(1.0);
                        crate::js_json::push_str(&mut out, u.as_str());
                    }
                    None => out.push(0.0),
                }
                match s.hotkey {
                    Some(h) => {
                        out.push(1.0);
                        crate::js_json::push_str(&mut out, h);
                    }
                    None => out.push(0.0),
                }
                match s.bullets {
                    Some(bs) => {
                        out.push(bs.len() as f64);
                        for b in bs {
                            crate::js_json::push_str(&mut out, b);
                        }
                    }
                    None => out.push(0.0),
                }
            }
            out.push(STEP_DONE_LINGER_TICKS);
        }
        0 => {
            // Flat script replay: [n, (opKind, ctx)*n] -> the concatenated
            // observable dumps (the TS capture emits no length prefix).
            let mut i = 0usize;
            let n = args[i] as usize;
            i += 1;
            let mut p = TutorialProgress::new();
            for _ in 0..n {
                let op = args[i] as u8;
                i += 1;
                let ctx = read_ctx(args, &mut i);
                match op {
                    1 => p.update(&ctx),
                    2 => p.acknowledge(),
                    3 => p.skip(),
                    k => unreachable!("tutorial: unknown script op {k}"),
                }
                push_dump(&mut out, &p, &ctx);
            }
        }
        k => unreachable!("tutorial: unknown op kind {k}"),
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> TutorialContext {
        TutorialContext::default()
    }

    #[test]
    fn step_table_shape() {
        assert_eq!(TUTORIAL_STEPS.len(), 22);
        assert_eq!(TUTORIAL_STEPS[0].id, "spawn");
        assert_eq!(TUTORIAL_STEPS[21].id, "sam_info");
        assert_eq!(TUTORIAL_STEPS[5].id, "capture_tribes");
        assert_eq!(TUTORIAL_STEPS[6].unit, Some(UnitTypeName::City));
        assert_eq!(TUTORIAL_STEPS[8].bullets.unwrap().len(), 2);
        assert_eq!(STEP_DONE_LINGER_TICKS, 15.0);
    }

    #[test]
    fn manual_acknowledge_and_linger() {
        let mut p = TutorialProgress::new();
        let c = ctx();
        p.update(&c); // spawn not done -> stays on index 0
        assert_eq!(p.current().unwrap().id, "spawn");
        p.skip(); // -> attack_wilderness
        p.skip(); // -> troops (manual)
        assert_eq!(p.current().unwrap().id, "troops");
        p.acknowledge();
        assert!(p.step_done());
        assert_eq!(p.done_ticks, Some(0.0));
        // 14 more updates: doneTicks climbs 1..14, still lingering.
        for t in 1..15 {
            p.update(&c);
            assert_eq!(p.done_ticks, Some(t as f64));
        }
        p.update(&c); // the 15th: advance to troop_rate, ticks reset null
        assert_eq!(p.current().unwrap().id, "troop_rate");
        assert!(!p.step_done());
        // acknowledge on an already-done step is inert.
        p.acknowledge();
        p.acknowledge();
        assert_eq!(p.done_ticks, Some(0.0));
    }

    #[test]
    fn count_ctx_latches_first_spawned() {
        let mut p = TutorialProgress::new();
        let mut c = ctx();
        c.bots_exist = false;
        c.city_disabled = true; // capture_tribes + buy_city inert
        p.update(&c); // not spawned -> no latch
        // Inert under this ctx: capture_tribes, buy_city (cityDisabled),
        // propose_alliance, alliance_info (nationsExist false) -> 18 apply.
        assert_eq!(p.total(&c), 18.0);
        c.has_spawned = true;
        p.update(&c); // latch takes THIS snapshot
        let latched = p.count_ctx.clone().unwrap();
        assert!(latched.has_spawned);
        // Later live contexts can't move the latch: bots/nations revive.
        c.bots_exist = true;
        c.nations_exist = true;
        assert_eq!(p.total(&c), 18.0);
        assert_eq!(p.total(&latched), 18.0);
    }

    #[test]
    fn skip_advances_and_resets_ticks() {
        let mut p = TutorialProgress::new();
        let c = ctx();
        p.update(&c);
        p.acknowledge(); // spawn is NOT manual -> inert
        assert!(!p.step_done());
        for _ in 0..22 {
            p.skip();
        }
        assert!(p.finished());
        assert!(p.current().is_none());
        // skip past the end is inert.
        p.skip();
        assert_eq!(p.index, 22);
        // Default ctx: capture_tribes / propose_alliance / alliance_info
        // inert -> 19 applicable.
        assert_eq!(p.position(&c), 19.0 + 1.0);
        assert_eq!(p.total(&c), 19.0);
    }

    #[test]
    fn capture_tribes_cost_gate() {
        let mut p = TutorialProgress::new();
        let mut c = ctx();
        c.bots_exist = true;
        c.city_disabled = false;
        // Skip to capture_tribes: spawn done, attack done.
        c.has_spawned = true;
        c.in_spawn_phase = false;
        p.update(&c);
        assert_eq!(p.current().unwrap().id, "spawn");
        assert!(p.step_done());
        // burn the linger.
        for _ in 0..15 {
            p.update(&c);
        }
        c.attacking = true;
        p.update(&c); // attack_wilderness done immediately? update order:
        // after the linger advance the while/isDone re-arm sees attacking ->
        // done at once. Burn 15 again.
        for _ in 0..15 {
            p.update(&c);
        }
        assert_eq!(p.current().unwrap().id, "troops");
        p.acknowledge();
        for _ in 0..15 {
            p.update(&c);
        }
        assert_eq!(p.current().unwrap().id, "troop_rate");
        p.acknowledge();
        for _ in 0..15 {
            p.update(&c);
        }
        assert_eq!(p.current().unwrap().id, "attack_ratio");
        c.attack_ratio_moved = true;
        for _ in 0..16 {
            p.update(&c);
        }
        assert_eq!(p.current().unwrap().id, "capture_tribes");
        // cityCost null and cities 0 -> not done.
        assert!(!p.step_done());
        // gold >= cityCost -> done.
        c.city_cost = Some(100.0);
        c.gold = 99.0;
        p.update(&c);
        assert!(!p.step_done());
        c.gold = 100.0;
        p.update(&c);
        assert!(p.step_done());
    }

    #[test]
    fn applies_skip_loop() {
        // A context where every gated step is inert: only the ungated 5
        // steps survive (spawn, attack_wilderness, troops, troop_rate,
        // attack_ratio).
        let mut p = TutorialProgress::new();
        let c = ctx();
        p.update(&c);
        // Walk the surviving steps by satisfying each.
        // spawn is not done (hasSpawned false) -> skip past.
        p.skip(); // attack_wilderness
        p.skip(); // troops
        assert_eq!(p.current().unwrap().id, "troops");
        // The while-loop only runs inside update(); with all gates false the
        // gated steps never appear.
        let mut c2 = ctx();
        c2.has_spawned = true;
        c2.attacking = true;
        c2.attack_ratio_moved = true;
        // Every gated step inert: all *_disabled true, no bots / nations.
        c2.bots_exist = false;
        c2.nations_exist = false;
        c2.city_disabled = true;
        c2.port_disabled = true;
        c2.defense_post_disabled = true;
        c2.factory_disabled = true;
        c2.warship_disabled = true;
        c2.silo_disabled = true;
        c2.atom_disabled = true;
        c2.hydrogen_disabled = true;
        c2.mirv_disabled = true;
        c2.sam_disabled = true;
        c2.boats_disabled = true;
        c2.alliances_disabled = true;
        let mut p2 = TutorialProgress::new();
        let ids: Vec<&str> = {
            let mut v = Vec::new();
            for _ in 0..200 {
                p2.update(&c2);
                if let Some(s) = p2.current() {
                    if !v.contains(&s.id) {
                        v.push(s.id);
                    }
                    if s.manual {
                        p2.acknowledge();
                    }
                } else {
                    break;
                }
            }
            v
        };
        assert_eq!(
            ids,
            ["spawn", "attack_wilderness", "troops", "troop_rate", "attack_ratio"]
        );
    }

    #[test]
    fn run_op_script_replay_wire() {
        // kind 0: two update ops then an acknowledge.
        let mut args = vec![3.0];
        for op in [1u8, 1, 2] {
            args.push(op as f64);
            push_ctx(&mut args, &ctx());
        }
        let res = run_op(0, &args);
        // update on the empty ctx: spawn applies, isDone false -> still on
        // spawn, pending; acknowledge inert (spawn not manual).
        assert_eq!(res[0], 1.0); // current present
        // last dump = still on spawn: tag + push_str(5 chars) + finished /
        // stepDone / position / total = 11 tokens.
        let last = res.len() - 11;
        assert_eq!(res[last], 1.0); // current present
        assert_eq!(res[last + 1], 5.0); // "spawn"
        assert_eq!(res[last + 2], 115.0); // 's'
        let table = run_op(5, &[]);
        assert_eq!(table[0], 22.0);
        assert_eq!(table[table.len() - 1], 15.0);
    }
}
