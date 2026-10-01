//! `src/core/game/GameUpdateUtils.ts` — per-player `PlayerUpdate` diffing,
//! in-place state merging and attack-troop-delta packing.
//!
//! Two modelling notes that keep the port bit-exact:
//!
//! * **Reference equality.** Every comparator starts with `a === b`, which in
//!   JS is true for the *same array object* even when it contains `NaN`
//!   (where a structural walk would report inequality). The capture assigns a
//!   `refid` token per distinct JS array; two fields sharing one object share
//!   one refid, and the Rust comparators check `refid` first — same refid is
//!   the `a === b` fast path, `None`/`None` is `undefined === undefined`.
//! * **Three-state fields.** `prev.x === next.x` distinguishes `undefined`
//!   from `null` (they are `!==` to each other), so optional fields are
//!   `Option<Option<T>>`: outer `None` = absent, inner `None` = `null`.
//!   Primitive equality then uses Rust's `==`, which matches JS `===` for
//!   numbers (`NaN == NaN` false, `-0 == 0` true) and `undefined`/`null`
//!   identity.
//!
//! The wire token streams (args / res) are documented on [`run_op`].

/// Direction lane of a `packedAttackUpdates` quad: the owner's
/// `outgoingAttacks` array.
pub const ATTACK_DELTA_OUTGOING: f64 = 0.0;
/// Direction lane for `incomingAttacks`.
pub const ATTACK_DELTA_INCOMING: f64 = 1.0;

/// `GameUpdateType.Player` — the constant the diff always stamps.
pub const GAME_UPDATE_TYPE_PLAYER: f64 = 2.0;

#[derive(Clone, Debug)]
pub struct AttackUpdate {
    pub attacker_id: f64,
    pub target_id: f64,
    pub troops: f64,
    pub id: String,
    pub retreating: bool,
}

#[derive(Clone, Debug)]
pub struct AllianceView {
    pub id: f64,
    pub other: String,
    pub created_at: f64,
    pub expires_at: f64,
    pub has_extension_request: bool,
}

#[derive(Clone, Debug)]
pub struct EmojiMessage {
    pub message: String,
    pub sender_id: f64,
    /// `number | "AllPlayers"` — `None` models the `"AllPlayers"` string
    /// (the only legal non-number value of the union); `Some(NaN)` vs
    /// `Some(NaN)` compares unequal exactly like JS `!==`.
    pub recipient_id: Option<f64>,
    pub created_at: f64,
}

/// An array-valued `PlayerUpdate` field: the capture's reference id (models
/// JS `a === b`) plus the elements.
#[derive(Clone, Debug)]
pub struct Arr<T> {
    pub refid: f64,
    pub items: Vec<T>,
}

type Opt3<T> = Option<Option<T>>;

/// The subset of `PlayerUpdate` the ported functions touch. `type` and
/// `nameViewData` are never read; fields the diff deliberately skips
/// (`tilesOwned` / `gold` / `troops` / `goldEarned`) are still carried
/// because `applyStateUpdate` reads them.
#[derive(Clone, Debug)]
pub struct PlayerUpdate {
    pub id: String,
    pub client_id: Opt3<String>,
    pub name: Opt3<String>,
    pub display_name: Opt3<String>,
    pub clan_tag: Opt3<String>,
    pub nation_flag: Opt3<String>,
    pub team: Opt3<String>,
    pub small_id: Opt3<f64>,
    pub player_type: Opt3<String>,
    pub is_alive: Opt3<bool>,
    pub is_disconnected: Opt3<bool>,
    pub killed_by: Opt3<String>,
    pub death_position: Opt3<f64>,
    pub tiles_owned: Opt3<f64>,
    pub gold: Opt3<f64>,
    pub trade_gold: Opt3<f64>,
    pub train_gold: Opt3<f64>,
    pub piracy_gold: Opt3<f64>,
    pub gold_earned: Opt3<f64>,
    pub troops: Opt3<f64>,
    pub allies: Option<Arr<f64>>,
    pub embargoes: Option<Arr<String>>,
    pub is_traitor: Opt3<bool>,
    pub traitor_remaining_ticks: Opt3<f64>,
    pub in_doomsday_clock: Opt3<bool>,
    pub is_decaying: Opt3<bool>,
    pub marked_doomsday_clock_tick: Opt3<f64>,
    pub targets: Option<Arr<f64>>,
    pub outgoing_emojis: Option<Arr<EmojiMessage>>,
    pub outgoing_attacks: Option<Arr<AttackUpdate>>,
    pub incoming_attacks: Option<Arr<AttackUpdate>>,
    pub outgoing_alliance_requests: Option<Arr<String>>,
    pub alliances: Option<Arr<AllianceView>>,
    pub has_spawned: Opt3<bool>,
    pub spawn_tile: Opt3<f64>,
    pub betrayals: Opt3<f64>,
    pub last_delete_unit_tick: Opt3<f64>,
    pub is_lobby_creator: Opt3<bool>,
}

/// The subset of the client `PlayerState` that `applyStateUpdate` writes.
/// Primitives are three-state so a `null` assignment on the wire is
/// observable; arrays lose their refid (a merge either detaches via `slice()`
/// or shares content — only content is compared).
#[derive(Clone, Debug)]
pub struct PlayerState {
    pub is_alive: Opt3<bool>,
    pub is_disconnected: Opt3<bool>,
    pub killed_by: Opt3<String>,
    pub death_position: Opt3<f64>,
    pub tiles_owned: Opt3<f64>,
    pub gold: Opt3<f64>,
    pub trade_gold: Opt3<f64>,
    pub train_gold: Opt3<f64>,
    pub piracy_gold: Opt3<f64>,
    pub gold_earned: Opt3<f64>,
    pub troops: Opt3<f64>,
    pub is_traitor: Opt3<bool>,
    pub traitor_remaining_ticks: Opt3<f64>,
    pub in_doomsday_clock: Opt3<bool>,
    pub marked_doomsday_clock_tick: Opt3<f64>,
    pub is_decaying: Opt3<bool>,
    pub betrayals: Opt3<f64>,
    pub has_spawned: Opt3<bool>,
    pub spawn_tile: Opt3<f64>,
    pub last_delete_unit_tick: Opt3<f64>,
    pub allies: Option<Vec<f64>>,
    pub targets: Option<Vec<f64>>,
    pub outgoing_alliance_requests: Option<Vec<String>>,
    pub outgoing_attacks: Option<Vec<AttackUpdate>>,
    pub incoming_attacks: Option<Vec<AttackUpdate>>,
    pub alliances: Option<Vec<AllianceView>>,
    pub outgoing_emojis: Option<Vec<EmojiMessage>>,
}

// ---------------------------------------------------------------------------
// comparators (private in the TS module; exercised through the public fns)
// ---------------------------------------------------------------------------

/// `numberArrayEqual`: ref-equality first, then length + per-element `!==`.
/// Rust `f64 !=` matches JS `!==` exactly (NaN vs NaN unequal, -0 vs 0 equal).
fn number_array_equal(a: &Option<Arr<f64>>, b: &Option<Arr<f64>>) -> bool {
    match (a, b) {
        (None, None) => true, // undefined === undefined
        (Some(x), Some(y)) => {
            if x.refid == y.refid {
                return true;
            }
            if x.items.len() != y.items.len() {
                return false;
            }
            x.items.iter().zip(y.items.iter()).all(|(p, q)| p == q)
        }
        _ => false,
    }
}

fn string_array_equal(a: &Option<Arr<String>>, b: &Option<Arr<String>>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(x), Some(y)) => {
            if x.refid == y.refid {
                return true;
            }
            if x.items.len() != y.items.len() {
                return false;
            }
            x.items.iter().zip(y.items.iter()).all(|(p, q)| p == q)
        }
        _ => false,
    }
}

/// `stringSetEqual`: same size and mutual membership (order-independent).
fn string_set_equal(a: &Option<Arr<String>>, b: &Option<Arr<String>>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(x), Some(y)) => {
            if x.refid == y.refid {
                return true;
            }
            if x.items.len() != y.items.len() {
                return false;
            }
            x.items.iter().all(|v| y.items.iter().any(|w| w == v))
        }
        _ => false,
    }
}

/// `attackArrayMembershipEqual`: same order, same attacker/target/id/
/// retreating — troop counts deliberately ignored.
fn attack_array_membership_equal(
    a: &Option<Arr<AttackUpdate>>,
    b: &Option<Arr<AttackUpdate>>,
) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(x), Some(y)) => {
            if x.refid == y.refid {
                return true;
            }
            if x.items.len() != y.items.len() {
                return false;
            }
            x.items.iter().zip(y.items.iter()).all(|(p, q)| {
                !(p.attacker_id != q.attacker_id
                    || p.target_id != q.target_id
                    || p.id != q.id
                    || p.retreating != q.retreating)
            })
        }
        _ => false,
    }
}

fn alliance_array_equal(a: &Option<Arr<AllianceView>>, b: &Option<Arr<AllianceView>>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(x), Some(y)) => {
            if x.refid == y.refid {
                return true;
            }
            if x.items.len() != y.items.len() {
                return false;
            }
            x.items.iter().zip(y.items.iter()).all(|(p, q)| {
                !(p.id != q.id
                    || p.other != q.other
                    || p.created_at != q.created_at
                    || p.expires_at != q.expires_at
                    || p.has_extension_request != q.has_extension_request)
            })
        }
        _ => false,
    }
}

fn emoji_array_equal(a: &Option<Arr<EmojiMessage>>, b: &Option<Arr<EmojiMessage>>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(x), Some(y)) => {
            if x.refid == y.refid {
                return true;
            }
            if x.items.len() != y.items.len() {
                return false;
            }
            x.items.iter().zip(y.items.iter()).all(|(p, q)| {
                !(p.message != q.message
                    || p.sender_id != q.sender_id
                    || p.recipient_id != q.recipient_id
                    || p.created_at != q.created_at)
            })
        }
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// diffPlayerUpdate
// ---------------------------------------------------------------------------

/// Field index of the diff output, in `setIfDifferent` call order.
const D_CLIENT_ID: f64 = 0.0;
const D_NAME: f64 = 1.0;
const D_DISPLAY_NAME: f64 = 2.0;
const D_CLAN_TAG: f64 = 3.0;
const D_NATION_FLAG: f64 = 4.0;
const D_TEAM: f64 = 5.0;
const D_SMALL_ID: f64 = 6.0;
const D_PLAYER_TYPE: f64 = 7.0;
const D_IS_ALIVE: f64 = 8.0;
const D_IS_DISCONNECTED: f64 = 9.0;
const D_KILLED_BY: f64 = 10.0;
const D_DEATH_POSITION: f64 = 11.0;
const D_TRADE_GOLD: f64 = 12.0;
const D_TRAIN_GOLD: f64 = 13.0;
const D_PIRACY_GOLD: f64 = 14.0;
const D_IS_TRAITOR: f64 = 15.0;
const D_TRAITOR_REMAINING_TICKS: f64 = 16.0;
const D_IN_DOOMSDAY_CLOCK: f64 = 17.0;
const D_MARKED_DOOMSDAY_CLOCK_TICK: f64 = 18.0;
const D_IS_DECAYING: f64 = 19.0;
const D_HAS_SPAWNED: f64 = 20.0;
const D_SPAWN_TILE: f64 = 21.0;
const D_BETRAYALS: f64 = 22.0;
const D_LAST_DELETE_UNIT_TICK: f64 = 23.0;
const D_IS_LOBBY_CREATOR: f64 = 24.0;
const D_ALLIES: f64 = 25.0;
const D_TARGETS: f64 = 26.0;
const D_OUTGOING_ALLIANCE_REQUESTS: f64 = 27.0;
const D_EMBARGOES: f64 = 28.0;
const D_OUTGOING_EMOJIS: f64 = 29.0;
const D_OUTGOING_ATTACKS: f64 = 30.0;
const D_INCOMING_ATTACKS: f64 = 31.0;
const D_ALLIANCES: f64 = 32.0;

/// `diffPlayerUpdate`: returns `null` (stream `[0]`) when every compared
/// field is equal, else `[1, id, (field_idx, value_enc)…]` in
/// `setIfDifferent` order. The fast path and the slow path use the same
/// comparator set, so the Rust port runs the comparisons once and emits the
/// changed fields in call order.
pub fn diff_player_update(prev: &PlayerUpdate, next: &PlayerUpdate) -> Vec<f64> {
    let mut out: Vec<f64> = Vec::new();

    let set = |out: &mut Vec<f64>, idx: f64, equal: bool, enc: &dyn Fn(&mut Vec<f64>)| {
        if !equal {
            out.push(idx);
            enc(out);
        }
    };

    // The fast-path / setIfDifferent comparison sequence, in order.
    let mut changed = false;
    let mut push_diff = |p: &mut Vec<f64>| {
        let eq_client = prev.client_id == next.client_id;
        let eq_name = prev.name == next.name;
        let eq_display = prev.display_name == next.display_name;
        let eq_clan = prev.clan_tag == next.clan_tag;
        let eq_flag = prev.nation_flag == next.nation_flag;
        let eq_team = prev.team == next.team;
        let eq_small = prev.small_id == next.small_id;
        let eq_ptype = prev.player_type == next.player_type;
        let eq_alive = prev.is_alive == next.is_alive;
        let eq_disc = prev.is_disconnected == next.is_disconnected;
        let eq_killed = prev.killed_by == next.killed_by;
        let eq_death = prev.death_position == next.death_position;
        let eq_trade = prev.trade_gold == next.trade_gold;
        let eq_train = prev.train_gold == next.train_gold;
        let eq_piracy = prev.piracy_gold == next.piracy_gold;
        let eq_traitor = prev.is_traitor == next.is_traitor;
        let eq_trt = prev.traitor_remaining_ticks == next.traitor_remaining_ticks;
        let eq_doom = prev.in_doomsday_clock == next.in_doomsday_clock;
        let eq_marked = prev.marked_doomsday_clock_tick == next.marked_doomsday_clock_tick;
        let eq_decay = prev.is_decaying == next.is_decaying;
        let eq_spawned = prev.has_spawned == next.has_spawned;
        let eq_stile = prev.spawn_tile == next.spawn_tile;
        let eq_betr = prev.betrayals == next.betrayals;
        let eq_ldut = prev.last_delete_unit_tick == next.last_delete_unit_tick;
        let eq_lobby = prev.is_lobby_creator == next.is_lobby_creator;
        let eq_allies = number_array_equal(&prev.allies, &next.allies);
        let eq_targets = number_array_equal(&prev.targets, &next.targets);
        let eq_oar = string_array_equal(
            &prev.outgoing_alliance_requests,
            &next.outgoing_alliance_requests,
        );
        let eq_emb = string_set_equal(&prev.embargoes, &next.embargoes);
        let eq_emo = emoji_array_equal(&prev.outgoing_emojis, &next.outgoing_emojis);
        let eq_oa = attack_array_membership_equal(&prev.outgoing_attacks, &next.outgoing_attacks);
        let eq_ia = attack_array_membership_equal(&prev.incoming_attacks, &next.incoming_attacks);
        let eq_all = alliance_array_equal(&prev.alliances, &next.alliances);

        if eq_client
            && eq_name
            && eq_display
            && eq_clan
            && eq_flag
            && eq_team
            && eq_small
            && eq_ptype
            && eq_alive
            && eq_disc
            && eq_killed
            && eq_death
            && eq_trade
            && eq_train
            && eq_piracy
            && eq_traitor
            && eq_trt
            && eq_doom
            && eq_marked
            && eq_decay
            && eq_spawned
            && eq_stile
            && eq_betr
            && eq_ldut
            && eq_lobby
            && eq_allies
            && eq_targets
            && eq_oar
            && eq_emb
            && eq_emo
            && eq_oa
            && eq_ia
            && eq_all
        {
            return; // fast path: no allocation, nothing changed
        }

        p.push(1.0);
        push_string(p, &next.id);
        set(p, D_CLIENT_ID, eq_client, &|o| push_opt3_string(o, &next.client_id));
        set(p, D_NAME, eq_name, &|o| push_opt3_string(o, &next.name));
        set(p, D_DISPLAY_NAME, eq_display, &|o| push_opt3_string(o, &next.display_name));
        set(p, D_CLAN_TAG, eq_clan, &|o| push_opt3_string(o, &next.clan_tag));
        set(p, D_NATION_FLAG, eq_flag, &|o| push_opt3_string(o, &next.nation_flag));
        set(p, D_TEAM, eq_team, &|o| push_opt3_string(o, &next.team));
        set(p, D_SMALL_ID, eq_small, &|o| push_opt3_num(o, &next.small_id));
        set(p, D_PLAYER_TYPE, eq_ptype, &|o| push_opt3_string(o, &next.player_type));
        set(p, D_IS_ALIVE, eq_alive, &|o| push_opt3_bool(o, &next.is_alive));
        set(p, D_IS_DISCONNECTED, eq_disc, &|o| push_opt3_bool(o, &next.is_disconnected));
        set(p, D_KILLED_BY, eq_killed, &|o| push_opt3_string(o, &next.killed_by));
        set(p, D_DEATH_POSITION, eq_death, &|o| push_opt3_num(o, &next.death_position));
        set(p, D_TRADE_GOLD, eq_trade, &|o| push_opt3_num(o, &next.trade_gold));
        set(p, D_TRAIN_GOLD, eq_train, &|o| push_opt3_num(o, &next.train_gold));
        set(p, D_PIRACY_GOLD, eq_piracy, &|o| push_opt3_num(o, &next.piracy_gold));
        set(p, D_IS_TRAITOR, eq_traitor, &|o| push_opt3_bool(o, &next.is_traitor));
        set(p, D_TRAITOR_REMAINING_TICKS, eq_trt, &|o| {
            push_opt3_num(o, &next.traitor_remaining_ticks)
        });
        set(p, D_IN_DOOMSDAY_CLOCK, eq_doom, &|o| push_opt3_bool(o, &next.in_doomsday_clock));
        set(p, D_MARKED_DOOMSDAY_CLOCK_TICK, eq_marked, &|o| {
            push_opt3_num(o, &next.marked_doomsday_clock_tick)
        });
        set(p, D_IS_DECAYING, eq_decay, &|o| push_opt3_bool(o, &next.is_decaying));
        set(p, D_HAS_SPAWNED, eq_spawned, &|o| push_opt3_bool(o, &next.has_spawned));
        set(p, D_SPAWN_TILE, eq_stile, &|o| push_opt3_num(o, &next.spawn_tile));
        set(p, D_BETRAYALS, eq_betr, &|o| push_opt3_num(o, &next.betrayals));
        set(p, D_LAST_DELETE_UNIT_TICK, eq_ldut, &|o| {
            push_opt3_num(o, &next.last_delete_unit_tick)
        });
        set(p, D_IS_LOBBY_CREATOR, eq_lobby, &|o| push_opt3_bool(o, &next.is_lobby_creator));
        set(p, D_ALLIES, eq_allies, &|o| push_arr_num(o, &next.allies));
        set(p, D_TARGETS, eq_targets, &|o| push_arr_num(o, &next.targets));
        set(p, D_OUTGOING_ALLIANCE_REQUESTS, eq_oar, &|o| {
            push_arr_str(o, &next.outgoing_alliance_requests)
        });
        set(p, D_EMBARGOES, eq_emb, &|o| push_arr_str(o, &next.embargoes));
        set(p, D_OUTGOING_EMOJIS, eq_emo, &|o| push_arr_emoji(o, &next.outgoing_emojis));
        set(p, D_OUTGOING_ATTACKS, eq_oa, &|o| push_arr_attack(o, &next.outgoing_attacks));
        set(p, D_INCOMING_ATTACKS, eq_ia, &|o| push_arr_attack(o, &next.incoming_attacks));
        set(p, D_ALLIANCES, eq_all, &|o| push_arr_alliance(o, &next.alliances));
        changed = true;
    };
    push_diff(&mut out);

    if !changed {
        out.push(0.0);
    }
    out
}

// ---------------------------------------------------------------------------
// applyStateUpdate
// ---------------------------------------------------------------------------

/// JS `Number(x)` on the wire values we model: `null` coerces to `0`, a
/// number passes through (`NaN`/`-0` keep their bits).
fn js_number(x: Option<f64>) -> f64 {
    x.unwrap_or(0.0)
}

/// JS `Math.max(0, x)`: `NaN` wins over everything, `null`/negative/`-0`
/// collapse to `+0`, otherwise `x`.
fn js_max0(x: Option<f64>) -> f64 {
    let x = js_number(x);
    if x.is_nan() {
        f64::NAN
    } else if x <= 0.0 {
        0.0
    } else {
        x
    }
}

/// `applyStateUpdate`: merge the present fields of `pu` into `target` in
/// place. `undefined` means "no change"; `null` is assigned (or coerced by
/// `Number()` / `Math.max` where the TS does so). Array fields that go
/// through `.slice()` are detached (content copied).
pub fn apply_state_update(target: &mut PlayerState, pu: &PlayerUpdate) {
    if pu.is_alive.is_some() {
        target.is_alive = pu.is_alive;
    }
    if pu.is_disconnected.is_some() {
        target.is_disconnected = pu.is_disconnected;
    }
    if pu.killed_by.is_some() {
        target.killed_by = pu.killed_by.clone();
    }
    if pu.death_position.is_some() {
        target.death_position = pu.death_position;
    }
    if pu.tiles_owned.is_some() {
        target.tiles_owned = pu.tiles_owned;
    }
    if let Some(v) = pu.gold {
        target.gold = Some(Some(js_number(v)));
    }
    if let Some(v) = pu.trade_gold {
        target.trade_gold = Some(Some(js_number(v)));
    }
    if let Some(v) = pu.train_gold {
        target.train_gold = Some(Some(js_number(v)));
    }
    if let Some(v) = pu.piracy_gold {
        target.piracy_gold = Some(Some(js_number(v)));
    }
    if let Some(v) = pu.gold_earned {
        target.gold_earned = Some(Some(js_number(v)));
    }
    if pu.troops.is_some() {
        target.troops = pu.troops;
    }
    if pu.is_traitor.is_some() {
        target.is_traitor = pu.is_traitor;
    }
    if let Some(v) = pu.traitor_remaining_ticks {
        target.traitor_remaining_ticks = Some(Some(js_max0(v)));
    }
    if pu.in_doomsday_clock.is_some() {
        target.in_doomsday_clock = pu.in_doomsday_clock;
    }
    if pu.marked_doomsday_clock_tick.is_some() {
        target.marked_doomsday_clock_tick = pu.marked_doomsday_clock_tick;
    }
    if pu.is_decaying.is_some() {
        target.is_decaying = pu.is_decaying;
    }
    if pu.betrayals.is_some() {
        target.betrayals = pu.betrayals;
    }
    if pu.has_spawned.is_some() {
        target.has_spawned = pu.has_spawned;
    }
    if pu.spawn_tile.is_some() {
        target.spawn_tile = pu.spawn_tile;
    }
    if pu.last_delete_unit_tick.is_some() {
        target.last_delete_unit_tick = pu.last_delete_unit_tick;
    }
    // slice()-detached arrays
    if let Some(a) = &pu.allies {
        target.allies = Some(a.items.clone());
    }
    if let Some(a) = &pu.targets {
        target.targets = Some(a.items.clone());
    }
    if let Some(a) = &pu.outgoing_alliance_requests {
        target.outgoing_alliance_requests = Some(a.items.clone());
    }
    // reference-assigned (content copied here; only content is observable)
    if let Some(a) = &pu.outgoing_attacks {
        target.outgoing_attacks = Some(a.items.clone());
    }
    if let Some(a) = &pu.incoming_attacks {
        target.incoming_attacks = Some(a.items.clone());
    }
    if let Some(a) = &pu.alliances {
        target.alliances = Some(a.items.clone());
    }
    if let Some(a) = &pu.outgoing_emojis {
        target.outgoing_emojis = Some(a.items.clone());
    }
}

// ---------------------------------------------------------------------------
// packAttackTroopDeltas
// ---------------------------------------------------------------------------

/// `packAttackTroopDeltas`: push `[owner, direction, i, troops]` quads for
/// each index whose troop count changed. No-op unless both arrays are present
/// and *not* the same reference, but membership-equal (so indexes line up).
pub fn pack_attack_troop_deltas(
    prev: &Option<Arr<AttackUpdate>>,
    next: &Option<Arr<AttackUpdate>>,
    owner_small_id: f64,
    direction: f64,
    out: &mut Vec<f64>,
) {
    let (p, n) = match (prev, next) {
        (Some(p), Some(n)) if p.refid != n.refid => (p, n),
        _ => return, // prev === next || !prev || !next
    };
    if !attack_array_membership_equal(prev, next) {
        return;
    }
    for i in 0..n.items.len() {
        if p.items[i].troops != n.items[i].troops {
            out.push(owner_small_id);
            out.push(direction);
            out.push(i as f64);
            out.push(n.items[i].troops);
        }
    }
}

// ---------------------------------------------------------------------------
// token codecs (shared with tools/gen_vectors.mjs / run_wasm_parity.mjs)
// ---------------------------------------------------------------------------

fn push_string(out: &mut Vec<f64>, s: &str) {
    let units: Vec<u16> = s.encode_utf16().collect();
    out.push(units.len() as f64);
    out.extend(units.iter().map(|&u| u as f64));
}

fn push_opt3_string(out: &mut Vec<f64>, v: &Opt3<String>) {
    match v {
        None => out.push(0.0),
        Some(None) => out.push(1.0),
        Some(Some(s)) => {
            out.push(2.0);
            push_string(out, s);
        }
    }
}

fn push_opt3_num(out: &mut Vec<f64>, v: &Opt3<f64>) {
    match v {
        None => out.push(0.0),
        Some(None) => out.push(1.0),
        Some(Some(x)) => {
            out.push(2.0);
            out.push(*x);
        }
    }
}

fn push_opt3_bool(out: &mut Vec<f64>, v: &Opt3<bool>) {
    match v {
        None => out.push(0.0),
        Some(None) => out.push(1.0),
        Some(Some(b)) => {
            out.push(2.0);
            out.push(if *b { 1.0 } else { 0.0 });
        }
    }
}

fn push_arr_num(out: &mut Vec<f64>, a: &Option<Arr<f64>>) {
    match a {
        None => out.push(0.0),
        Some(x) => {
            out.push(2.0);
            out.push(x.refid);
            out.push(x.items.len() as f64);
            out.extend(x.items.iter());
        }
    }
}

fn push_arr_str(out: &mut Vec<f64>, a: &Option<Arr<String>>) {
    match a {
        None => out.push(0.0),
        Some(x) => {
            out.push(2.0);
            out.push(x.refid);
            out.push(x.items.len() as f64);
            for s in &x.items {
                push_string(out, s);
            }
        }
    }
}

fn push_attack(out: &mut Vec<f64>, a: &AttackUpdate) {
    out.push(a.attacker_id);
    out.push(a.target_id);
    out.push(a.troops);
    push_string(out, &a.id);
    out.push(if a.retreating { 1.0 } else { 0.0 });
}

fn push_arr_attack(out: &mut Vec<f64>, a: &Option<Arr<AttackUpdate>>) {
    match a {
        None => out.push(0.0),
        Some(x) => {
            out.push(2.0);
            out.push(x.refid);
            out.push(x.items.len() as f64);
            for e in &x.items {
                push_attack(out, e);
            }
        }
    }
}

fn push_alliance(out: &mut Vec<f64>, a: &AllianceView) {
    out.push(a.id);
    push_string(out, &a.other);
    out.push(a.created_at);
    out.push(a.expires_at);
    out.push(if a.has_extension_request { 1.0 } else { 0.0 });
}

fn push_arr_alliance(out: &mut Vec<f64>, a: &Option<Arr<AllianceView>>) {
    match a {
        None => out.push(0.0),
        Some(x) => {
            out.push(2.0);
            out.push(x.refid);
            out.push(x.items.len() as f64);
            for e in &x.items {
                push_alliance(out, e);
            }
        }
    }
}

fn push_emoji(out: &mut Vec<f64>, e: &EmojiMessage) {
    push_string(out, &e.message);
    out.push(e.sender_id);
    match e.recipient_id {
        None => out.push(0.0), // "AllPlayers"
        Some(x) => {
            out.push(1.0);
            out.push(x);
        }
    }
    out.push(e.created_at);
}

fn push_arr_emoji(out: &mut Vec<f64>, a: &Option<Arr<EmojiMessage>>) {
    match a {
        None => out.push(0.0),
        Some(x) => {
            out.push(2.0);
            out.push(x.refid);
            out.push(x.items.len() as f64);
            for e in &x.items {
                push_emoji(out, e);
            }
        }
    }
}

/// State arrays are pushed without a refid (post-merge content only).
fn push_state_arr_num(out: &mut Vec<f64>, a: &Option<Vec<f64>>) {
    match a {
        None => out.push(0.0),
        Some(x) => {
            out.push(1.0);
            out.push(x.len() as f64);
            out.extend(x.iter());
        }
    }
}

fn push_state_arr_str(out: &mut Vec<f64>, a: &Option<Vec<String>>) {
    match a {
        None => out.push(0.0),
        Some(x) => {
            out.push(1.0);
            out.push(x.len() as f64);
            for s in x {
                push_string(out, s);
            }
        }
    }
}

fn push_state_arr_attack(out: &mut Vec<f64>, a: &Option<Vec<AttackUpdate>>) {
    match a {
        None => out.push(0.0),
        Some(x) => {
            out.push(1.0);
            out.push(x.len() as f64);
            for e in x {
                push_attack(out, e);
            }
        }
    }
}

fn push_state_arr_alliance(out: &mut Vec<f64>, a: &Option<Vec<AllianceView>>) {
    match a {
        None => out.push(0.0),
        Some(x) => {
            out.push(1.0);
            out.push(x.len() as f64);
            for e in x {
                push_alliance(out, e);
            }
        }
    }
}

fn push_state_arr_emoji(out: &mut Vec<f64>, a: &Option<Vec<EmojiMessage>>) {
    match a {
        None => out.push(0.0),
        Some(x) => {
            out.push(1.0);
            out.push(x.len() as f64);
            for e in x {
                push_emoji(out, e);
            }
        }
    }
}

fn push_state(out: &mut Vec<f64>, s: &PlayerState) {
    push_opt3_bool(out, &s.is_alive);
    push_opt3_bool(out, &s.is_disconnected);
    push_opt3_string(out, &s.killed_by);
    push_opt3_num(out, &s.death_position);
    push_opt3_num(out, &s.tiles_owned);
    push_opt3_num(out, &s.gold);
    push_opt3_num(out, &s.trade_gold);
    push_opt3_num(out, &s.train_gold);
    push_opt3_num(out, &s.piracy_gold);
    push_opt3_num(out, &s.gold_earned);
    push_opt3_num(out, &s.troops);
    push_opt3_bool(out, &s.is_traitor);
    push_opt3_num(out, &s.traitor_remaining_ticks);
    push_opt3_bool(out, &s.in_doomsday_clock);
    push_opt3_num(out, &s.marked_doomsday_clock_tick);
    push_opt3_bool(out, &s.is_decaying);
    push_opt3_num(out, &s.betrayals);
    push_opt3_bool(out, &s.has_spawned);
    push_opt3_num(out, &s.spawn_tile);
    push_opt3_num(out, &s.last_delete_unit_tick);
    push_state_arr_num(out, &s.allies);
    push_state_arr_num(out, &s.targets);
    push_state_arr_str(out, &s.outgoing_alliance_requests);
    push_state_arr_attack(out, &s.outgoing_attacks);
    push_state_arr_attack(out, &s.incoming_attacks);
    push_state_arr_alliance(out, &s.alliances);
    push_state_arr_emoji(out, &s.outgoing_emojis);
}

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
    fn string(&mut self) -> String {
        let len = self.u();
        let units: Vec<u16> = (0..len).map(|_| self.f() as u16).collect();
        String::from_utf16_lossy(&units)
    }
    fn opt3_string(&mut self) -> Opt3<String> {
        match self.f() {
            0.0 => None,
            1.0 => Some(None),
            _ => Some(Some(self.string())),
        }
    }
    fn opt3_num(&mut self) -> Opt3<f64> {
        match self.f() {
            0.0 => None,
            1.0 => Some(None),
            _ => Some(Some(self.f())),
        }
    }
    fn opt3_bool(&mut self) -> Opt3<bool> {
        match self.f() {
            0.0 => None,
            1.0 => Some(None),
            _ => Some(Some(self.f() != 0.0)),
        }
    }
    fn arr_num(&mut self) -> Option<Arr<f64>> {
        match self.f() {
            0.0 => None,
            _ => {
                let refid = self.f();
                let len = self.u();
                Some(Arr { refid, items: (0..len).map(|_| self.f()).collect() })
            }
        }
    }
    fn arr_str(&mut self) -> Option<Arr<String>> {
        match self.f() {
            0.0 => None,
            _ => {
                let refid = self.f();
                let len = self.u();
                Some(Arr { refid, items: (0..len).map(|_| self.string()).collect() })
            }
        }
    }
    fn attack(&mut self) -> AttackUpdate {
        let attacker_id = self.f();
        let target_id = self.f();
        let troops = self.f();
        let id = self.string();
        let retreating = self.f() != 0.0;
        AttackUpdate { attacker_id, target_id, troops, id, retreating }
    }
    fn arr_attack(&mut self) -> Option<Arr<AttackUpdate>> {
        match self.f() {
            0.0 => None,
            _ => {
                let refid = self.f();
                let len = self.u();
                Some(Arr { refid, items: (0..len).map(|_| self.attack()).collect() })
            }
        }
    }
    fn alliance(&mut self) -> AllianceView {
        let id = self.f();
        let other = self.string();
        let created_at = self.f();
        let expires_at = self.f();
        let has_extension_request = self.f() != 0.0;
        AllianceView { id, other, created_at, expires_at, has_extension_request }
    }
    fn arr_alliance(&mut self) -> Option<Arr<AllianceView>> {
        match self.f() {
            0.0 => None,
            _ => {
                let refid = self.f();
                let len = self.u();
                Some(Arr { refid, items: (0..len).map(|_| self.alliance()).collect() })
            }
        }
    }
    fn emoji(&mut self) -> EmojiMessage {
        let message = self.string();
        let sender_id = self.f();
        let recipient_id = if self.f() == 0.0 { None } else { Some(self.f()) };
        let created_at = self.f();
        EmojiMessage { message, sender_id, recipient_id, created_at }
    }
    fn arr_emoji(&mut self) -> Option<Arr<EmojiMessage>> {
        match self.f() {
            0.0 => None,
            _ => {
                let refid = self.f();
                let len = self.u();
                Some(Arr { refid, items: (0..len).map(|_| self.emoji()).collect() })
            }
        }
    }

    /// Decode a `PlayerUpdate` (interface field order, `type`/`nameViewData`
    /// omitted).
    fn player_update(&mut self) -> PlayerUpdate {
        PlayerUpdate {
            id: self.string(),
            client_id: self.opt3_string(),
            name: self.opt3_string(),
            display_name: self.opt3_string(),
            clan_tag: self.opt3_string(),
            nation_flag: self.opt3_string(),
            team: self.opt3_string(),
            small_id: self.opt3_num(),
            player_type: self.opt3_string(),
            is_alive: self.opt3_bool(),
            is_disconnected: self.opt3_bool(),
            killed_by: self.opt3_string(),
            death_position: self.opt3_num(),
            tiles_owned: self.opt3_num(),
            gold: self.opt3_num(),
            trade_gold: self.opt3_num(),
            train_gold: self.opt3_num(),
            piracy_gold: self.opt3_num(),
            gold_earned: self.opt3_num(),
            troops: self.opt3_num(),
            allies: self.arr_num(),
            embargoes: self.arr_str(),
            is_traitor: self.opt3_bool(),
            traitor_remaining_ticks: self.opt3_num(),
            in_doomsday_clock: self.opt3_bool(),
            is_decaying: self.opt3_bool(),
            marked_doomsday_clock_tick: self.opt3_num(),
            targets: self.arr_num(),
            outgoing_emojis: self.arr_emoji(),
            outgoing_attacks: self.arr_attack(),
            incoming_attacks: self.arr_attack(),
            outgoing_alliance_requests: self.arr_str(),
            alliances: self.arr_alliance(),
            has_spawned: self.opt3_bool(),
            spawn_tile: self.opt3_num(),
            betrayals: self.opt3_num(),
            last_delete_unit_tick: self.opt3_num(),
            is_lobby_creator: self.opt3_bool(),
        }
    }

    /// Decode a `PlayerState` (the apply-touched subset, in apply order).
    fn player_state(&mut self) -> PlayerState {
        PlayerState {
            is_alive: self.opt3_bool(),
            is_disconnected: self.opt3_bool(),
            killed_by: self.opt3_string(),
            death_position: self.opt3_num(),
            tiles_owned: self.opt3_num(),
            gold: self.opt3_num(),
            trade_gold: self.opt3_num(),
            train_gold: self.opt3_num(),
            piracy_gold: self.opt3_num(),
            gold_earned: self.opt3_num(),
            troops: self.opt3_num(),
            is_traitor: self.opt3_bool(),
            traitor_remaining_ticks: self.opt3_num(),
            in_doomsday_clock: self.opt3_bool(),
            marked_doomsday_clock_tick: self.opt3_num(),
            is_decaying: self.opt3_bool(),
            betrayals: self.opt3_num(),
            has_spawned: self.opt3_bool(),
            spawn_tile: self.opt3_num(),
            last_delete_unit_tick: self.opt3_num(),
            allies: self.state_arr_num(),
            targets: self.state_arr_num(),
            outgoing_alliance_requests: self.state_arr_str(),
            outgoing_attacks: self.state_arr_attack(),
            incoming_attacks: self.state_arr_attack(),
            alliances: self.state_arr_alliance(),
            outgoing_emojis: self.state_arr_emoji(),
        }
    }
    fn state_arr_num(&mut self) -> Option<Vec<f64>> {
        if self.f() == 0.0 {
            None
        } else {
            let len = self.u();
            Some((0..len).map(|_| self.f()).collect())
        }
    }
    fn state_arr_str(&mut self) -> Option<Vec<String>> {
        if self.f() == 0.0 {
            None
        } else {
            let len = self.u();
            Some((0..len).map(|_| self.string()).collect())
        }
    }
    fn state_arr_attack(&mut self) -> Option<Vec<AttackUpdate>> {
        if self.f() == 0.0 {
            None
        } else {
            let len = self.u();
            Some((0..len).map(|_| self.attack()).collect())
        }
    }
    fn state_arr_alliance(&mut self) -> Option<Vec<AllianceView>> {
        if self.f() == 0.0 {
            None
        } else {
            let len = self.u();
            Some((0..len).map(|_| self.alliance()).collect())
        }
    }
    fn state_arr_emoji(&mut self) -> Option<Vec<EmojiMessage>> {
        if self.f() == 0.0 {
            None
        } else {
            let len = self.u();
            Some((0..len).map(|_| self.emoji()).collect())
        }
    }
}

/// `kind`: 0 = `diffPlayerUpdate(prev, next)` → res `[0]` (null) or
/// `[1, id, (field_idx, value)…]`; 1 = `applyStateUpdate(target, pu)` → res
/// is the post-merge `PlayerState` stream (apply order, arrays without
/// refid); 2 = `packAttackTroopDeltas(prev, next, owner, direction)` → res
/// `[len, …quads]`.
///
/// Array fields are `[0]` (undefined) or `[2, refid, len, elems…]`;
/// three-state fields `[0]/[1]/[2, v]`; strings `[len, u0, …]`. The diff
/// stamps a changed array with `next`'s refid — the diff object holds that
/// array by reference, so its identity is already pinned by the input stream.
pub fn run_op(kind: u8, args: &[f64]) -> Vec<f64> {
    let mut c = Cur(args, 0);
    let mut out = Vec::new();
    match kind {
        0 => {
            let prev = c.player_update();
            let next = c.player_update();
            out = diff_player_update(&prev, &next);
        }
        1 => {
            let mut target = c.player_state();
            let pu = c.player_update();
            apply_state_update(&mut target, &pu);
            push_state(&mut out, &target);
        }
        _ => {
            let prev = c.arr_attack();
            let next = c.arr_attack();
            let owner = c.f();
            let dir = c.f();
            pack_attack_troop_deltas(&prev, &next, owner, dir, &mut out);
            let len = out.len();
            let mut res = Vec::with_capacity(len + 1);
            res.push(len as f64);
            res.extend(out);
            out = res;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pu(id: &str) -> PlayerUpdate {
        PlayerUpdate {
            id: id.to_string(),
            client_id: None,
            name: None,
            display_name: None,
            clan_tag: None,
            nation_flag: None,
            team: None,
            small_id: None,
            player_type: None,
            is_alive: None,
            is_disconnected: None,
            killed_by: None,
            death_position: None,
            tiles_owned: None,
            gold: None,
            trade_gold: None,
            train_gold: None,
            piracy_gold: None,
            gold_earned: None,
            troops: None,
            allies: None,
            embargoes: None,
            is_traitor: None,
            traitor_remaining_ticks: None,
            in_doomsday_clock: None,
            is_decaying: None,
            marked_doomsday_clock_tick: None,
            targets: None,
            outgoing_emojis: None,
            outgoing_attacks: None,
            incoming_attacks: None,
            outgoing_alliance_requests: None,
            alliances: None,
            has_spawned: None,
            spawn_tile: None,
            betrayals: None,
            last_delete_unit_tick: None,
            is_lobby_creator: None,
        }
    }

    #[test]
    fn diff_equal_is_null() {
        let a = pu("p1");
        let b = pu("p1");
        assert_eq!(diff_player_update(&a, &b), vec![0.0]);
    }

    #[test]
    fn diff_nan_primitive() {
        let mut a = pu("p1");
        let mut b = pu("p1");
        a.small_id = Some(Some(f64::NAN));
        b.small_id = Some(Some(f64::NAN));
        // NaN === NaN is false -> the field is emitted despite equal bits.
        let got = diff_player_update(&a, &b);
        assert_eq!(got[0], 1.0);
    }

    #[test]
    fn diff_same_ref_nan_array_equal() {
        let mut a = pu("p1");
        let mut b = pu("p1");
        let arr = Arr { refid: 7.0, items: vec![f64::NAN] };
        a.allies = Some(arr.clone());
        b.allies = Some(arr);
        assert_eq!(diff_player_update(&a, &b), vec![0.0]);
        b.allies = Some(Arr { refid: 8.0, items: vec![f64::NAN] });
        let got = diff_player_update(&a, &b);
        assert_eq!(got[0], 1.0);
    }

    #[test]
    fn apply_null_vs_undefined() {
        let mut t = PlayerState {
            is_alive: Some(Some(true)),
            is_disconnected: Some(Some(false)),
            killed_by: None,
            death_position: None,
            tiles_owned: Some(Some(1.0)),
            gold: Some(Some(1.0)),
            trade_gold: None,
            train_gold: None,
            piracy_gold: None,
            gold_earned: None,
            troops: Some(Some(2.0)),
            is_traitor: None,
            traitor_remaining_ticks: None,
            in_doomsday_clock: None,
            marked_doomsday_clock_tick: None,
            is_decaying: None,
            betrayals: None,
            has_spawned: None,
            spawn_tile: None,
            last_delete_unit_tick: None,
            allies: None,
            targets: None,
            outgoing_alliance_requests: None,
            outgoing_attacks: None,
            incoming_attacks: None,
            alliances: None,
            outgoing_emojis: None,
        };
        let mut pu_ = pu("p");
        pu_.gold = Some(None); // null -> Number(null) = 0
        pu_.traitor_remaining_ticks = Some(Some(-3.0)); // max(0,-3) = 0
        pu_.is_alive = Some(None); // null assigned as-is
        apply_state_update(&mut t, &pu_);
        assert_eq!(t.gold, Some(Some(0.0)));
        assert_eq!(t.traitor_remaining_ticks, Some(Some(0.0)));
        assert_eq!(t.is_alive, Some(None));
    }

    #[test]
    fn pack_requires_membership_equal() {
        let mk = |refid: f64, troops: &[f64]| -> Arr<AttackUpdate> {
            Arr {
                refid,
                items: troops
                    .iter()
                    .enumerate()
                    .map(|(i, &t)| AttackUpdate {
                        attacker_id: 1.0,
                        target_id: 2.0,
                        troops: t,
                        id: format!("a{i}"),
                        retreating: false,
                    })
                    .collect(),
            }
        };
        let prev = Some(mk(1.0, &[10.0, 20.0]));
        let next = Some(mk(2.0, &[11.0, 20.0]));
        let mut out = Vec::new();
        pack_attack_troop_deltas(&prev, &next, 5.0, ATTACK_DELTA_OUTGOING, &mut out);
        assert_eq!(out, vec![5.0, 0.0, 0.0, 11.0]);
        // same reference -> no-op
        let same = Some(mk(3.0, &[10.0]));
        let mut out2 = Vec::new();
        pack_attack_troop_deltas(&same, &same, 5.0, ATTACK_DELTA_INCOMING, &mut out2);
        assert!(out2.is_empty());
        // membership differs -> no-op
        let mut bad = mk(4.0, &[10.0]);
        bad.items[0].id = "zz".to_string();
        let mut out3 = Vec::new();
        pack_attack_troop_deltas(&prev, &Some(bad), 5.0, ATTACK_DELTA_OUTGOING, &mut out3);
        assert!(out3.is_empty());
    }
}
