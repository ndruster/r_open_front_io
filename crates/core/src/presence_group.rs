//! Port of `src/client/PresenceGroup.ts` — the per-game grouping token rules.
//!
//! Faithfulness notes (quirk list):
//!
//! * `groupTokenOf` gates on `message.type === "lobby_info" || === "start"`
//!   (STRICT string equality); every other type — and a message that carries
//!   no `groupToken` key at all — reads out as JS `undefined`.
//! * `loggableStartMessage` is `{ ...message }` (insertion-order copy) then
//!   `delete loggable.groupToken` — the key is REMOVED in place (the
//!   `privilege::resolveVerifiedJoin` delete precedent), so the dump shows
//!   every other key in the original order with `groupToken` gone.
//! * `GroupTokenTracker.accept` compares with STRICT `===` — `undefined ===
//!   undefined` is `true`, so accepting an undefined token onto a fresh
//!   tracker changes nothing and returns `false`.
//! * `presenceLobbyId` gates `config === undefined` STRICT, then reads
//!   `config.gameType` — a present-but-NULL config therefore THROWS a
//!   TypeError upstream (the capture records it as the `[99]` sentinel and the
//!   port models it with `presence_lobby_id` returning `None`); a non-object config (string / number)
//!   reads `.gameType` as `undefined`, fails the enum gate and returns the
//!   gameID. The second gate is `gameType === GameType.Public && gameMode !==
//!   GameMode.Team` (the string-enum values `"Public"` / `"Team"`).
//! * `withGroupToken` returns the SAME payload reference when the token is
//!   `undefined` (the shell diffs payloads; `{groupToken: undefined}` is a
//!   different object — the capture pins the identity through a `sameRef`
//!   flag); otherwise the spread appends `groupToken` as the LAST key.
//!
//! `GameMode` / `GameType` are the string enums ported in [`crate::game_ts`]
//! (`GameType.Public = "Public"`, `GameMode.Team = "Team"`).

use crate::js_json::{push_val, read_val, JsVal};

/// `GameType.Public`.
const GAME_TYPE_PUBLIC: &str = "Public";
/// `GameMode.Team`.
const GAME_MODE_TEAM: &str = "Team";

/// JS `===` over the codec domain the tracker / token gates see (strings,
/// undefined and null; object identity is never exercised — the capture
/// feeds scalars).
fn strict_eq(a: &JsVal, b: &JsVal) -> bool {
    match (a, b) {
        (JsVal::Absent, JsVal::Undef) | (JsVal::Undef, JsVal::Absent) => true,
        (JsVal::Absent, JsVal::Absent) | (JsVal::Undef, JsVal::Undef) | (JsVal::Null, JsVal::Null) => true,
        (JsVal::Bool(x), JsVal::Bool(y)) => x == y,
        (JsVal::Num(x), JsVal::Num(y)) => x == y,
        (JsVal::Str(x), JsVal::Str(y)) => x == y,
        _ => false,
    }
}

/// Read `obj[key]` as a JsVal; an absent key reads as JS `undefined`.
fn field<'a>(obj: &'a JsVal, key: &str) -> &'a JsVal {
    static UNDEF: JsVal = JsVal::Undef;
    match obj {
        JsVal::Obj(fields) => match fields.iter().find(|(k, _)| k == key) {
            Some((_, v)) => v,
            None => &UNDEF,
        },
        _ => &UNDEF,
    }
}

/// `groupTokenOf(message)` — the token the message carries, if any.
pub fn group_token_of(message: &JsVal) -> JsVal {
    let ty = field(message, "type");
    if matches!(ty, JsVal::Str(s) if s == "lobby_info") || matches!(ty, JsVal::Str(s) if s == "start")
    {
        field(message, "groupToken").clone()
    } else {
        JsVal::Undef
    }
}

/// `loggableStartMessage(message)` — `{...message}` then `delete .groupToken`.
pub fn loggable_start_message(message: &JsVal) -> JsVal {
    match message {
        JsVal::Obj(fields) => {
            let mut out = fields.clone();
            out.retain(|(k, _)| k != "groupToken");
            JsVal::Obj(out)
        }
        other => other.clone(),
    }
}

/// The ported `GroupTokenTracker`.
#[derive(Debug, Default)]
pub struct GroupTokenTracker {
    token: JsVal,
}

impl GroupTokenTracker {
    pub fn new() -> Self {
        Self { token: JsVal::Undef }
    }

    /// `current()`.
    pub fn current(&self) -> JsVal {
        self.token.clone()
    }

    /// `accept(token)` — true when this call CHANGED the tracked token.
    pub fn accept(&mut self, token: JsVal) -> bool {
        if strict_eq(&self.token, &token) {
            return false;
        }
        self.token = token;
        true
    }

    /// `clear()` — back to `undefined`.
    pub fn clear(&mut self) {
        self.token = JsVal::Undef;
    }
}

/// `presenceLobbyId(config, gameID)` — `None` models the upstream TypeError
/// thrown when a present-but-NULL config reaches `config.gameType` (the
/// STRICT `=== undefined` gate only stops undefined; primitives box and read
/// `.gameType` as undefined without throwing).
pub fn presence_lobby_id(config: &JsVal, game_id: &JsVal) -> Option<JsVal> {
    match config {
        JsVal::Undef | JsVal::Absent => return Some(JsVal::Undef),
        JsVal::Null => return None,
        _ => {}
    }
    let game_type = field(config, "gameType");
    let game_mode = field(config, "gameMode");
    if matches!(game_type, JsVal::Str(s) if s == GAME_TYPE_PUBLIC)
        && !(matches!(game_mode, JsVal::Str(s) if s == GAME_MODE_TEAM))
    {
        return Some(JsVal::Undef);
    }
    Some(game_id.clone())
}

/// `withGroupToken(payload, groupToken)` — `(result, same-reference-as-input)`.
pub fn with_group_token(payload: &JsVal, group_token: &JsVal) -> (JsVal, bool) {
    if matches!(group_token, JsVal::Undef | JsVal::Absent) {
        return (payload.clone(), true);
    }
    match payload {
        JsVal::Obj(fields) => {
            // `{ ...payload, groupToken }` — an EXISTING groupToken key is
            // overwritten in place (its original position survives), a new
            // one is appended last.
            let mut out = fields.clone();
            match out.iter_mut().find(|(k, _)| k == "groupToken") {
                Some(slot) => slot.1 = group_token.clone(),
                None => out.push(("groupToken".to_string(), group_token.clone())),
            }
            (JsVal::Obj(out), false)
        }
        other => (other.clone(), false),
    }
}

/// The capture harness: an op stream over one tracker instance (only the
/// tracker is stateful; the pure functions ride the same uniform replay).
#[derive(Debug, Default)]
pub struct RigHarness {
    tracker: GroupTokenTracker,
}

impl RigHarness {
    pub fn new() -> Self {
        Self { tracker: GroupTokenTracker::new() }
    }

    pub fn reset(&mut self) {
        *self = Self::new();
    }

    /// Run one op over the flat codec token stream. Kind table (see
    /// `tools/gen_vectors.mjs`):
    /// 0 groupTokenOf `[...codec(message)]` -> `[...codec(token)]`;
    /// 1 loggableStartMessage `[...codec(message)]` -> `[...codec(loggable)]`;
    /// 2 tracker.accept `[...codec(token)]` -> `[0|1]`;
    /// 3 tracker.current -> `[...codec(token)]`;
    /// 4 tracker.clear -> `[0]`;
    /// 5 presenceLobbyId `[...codec(config), ...codec(gameID)]` -> codec,
    ///   or `[99]` when a NULL config throws upstream;
    /// 6 withGroupToken `[...codec(payload), ...codec(groupToken)]` ->
    ///   `[sameRef, ...codec(result)]`.
    pub fn run_op(&mut self, kind: u8, args: &[f64]) -> Vec<f64> {
        let mut out = Vec::new();
        let mut i = 0usize;
        match kind {
            0 => {
                let message = read_val(args, &mut i);
                push_val(&mut out, &group_token_of(&message));
            }
            1 => {
                let message = read_val(args, &mut i);
                push_val(&mut out, &loggable_start_message(&message));
            }
            2 => {
                let token = read_val(args, &mut i);
                out.push(if self.tracker.accept(token) { 1.0 } else { 0.0 });
            }
            3 => push_val(&mut out, &self.tracker.current()),
            4 => {
                self.tracker.clear();
                out.push(0.0);
            }
            5 => {
                let config = read_val(args, &mut i);
                let game_id = read_val(args, &mut i);
                match presence_lobby_id(&config, &game_id) {
                    Some(v) => push_val(&mut out, &v),
                    // NULL config -> upstream TypeError reading .gameType;
                    // the capture records the [99] sentinel.
                    None => out.push(99.0),
                }
            }
            6 => {
                let payload = read_val(args, &mut i);
                let token = read_val(args, &mut i);
                let (result, same_ref) = with_group_token(&payload, &token);
                out.push(if same_ref { 1.0 } else { 0.0 });
                push_val(&mut out, &result);
            }
            k => unreachable!("presence_group harness: unknown op kind {k}"),
        }
        out
    }
}
