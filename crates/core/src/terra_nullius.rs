//! Port of `src/core/game/TerraNulliusImpl.ts`: the neutral-water player
//! stand-in. The class is four constant-return methods over an empty ctor —
//! `smallID()` is `0`, `clientID()` the literal string
//! `"TERRA_NULLIUS_CLIENT_ID"`, `id()` JS `null`, `isPlayer()` the literal
//! `false`. There is no state and no logic beyond those returns; the `Game`
//! / `Player` interfaces it implements are type-only and not ported.
//!
//! Faithfulness notes:
//!
//! * `id()` returns JS `null` — the token stream models it with the
//!   server_list `encOut` sentinel `[-1]` (a real client id / number is
//!   `[len, u0, ..]` or the value itself, never negative).
//! * `isPlayer(): false as const` — the return type pins the literal `false`;
//!   the capture encodes booleans as `0`/`1`.

/// The literal `clientID()` return.
pub const CLIENT_ID: &str = "TERRA_NULLIUS_CLIENT_ID";

/// `TerraNulliusImpl` — the stateless neutral (Terra Nullius) player facade.
#[derive(Clone, Copy, Debug, Default)]
pub struct TerraNulliusImpl;

impl TerraNulliusImpl {
    pub fn new() -> Self {
        TerraNulliusImpl
    }

    /// `smallID(): number` — the neutral's fixed small id.
    pub fn small_id(&self) -> f64 {
        0.0
    }

    /// `clientID(): ClientID` — the neutral's fixed client id string.
    pub fn client_id(&self) -> &'static str {
        CLIENT_ID
    }

    /// `id()` — JS `null` (the neutral has no game id). `None` models it.
    pub fn id(&self) -> Option<f64> {
        None
    }

    /// `isPlayer(): false` — the literal `false`.
    pub fn is_player(&self) -> bool {
        false
    }
}

// ---------------------------------------------------------------- vectors op
//
// A flat `f64` token runner shared by the golden replay and the wasm probe.
// Strings cross as `[len, u0, .. u(len-1)]` (UTF-16 code units).
//
// kind table (mirrors the capture):
//   0 [0] -> [0]              smallID
//   1 [1] -> [len, u0, ..]    clientID
//   2 [2] -> [-1]             id (JS null sentinel)
//   3 [3] -> [0|1]            isPlayer

pub fn run_op(kind: u8, _args: &[f64]) -> Vec<f64> {
    let tn = TerraNulliusImpl::new();
    match kind {
        0 => vec![tn.small_id()],
        1 => {
            let units: Vec<u16> = tn.client_id().encode_utf16().collect();
            let mut out = Vec::with_capacity(1 + units.len());
            out.push(units.len() as f64);
            out.extend(units.iter().map(|&u| u as f64));
            out
        }
        2 => vec![if tn.id().is_none() { -1.0 } else { 0.0 }],
        3 => vec![if tn.is_player() { 1.0 } else { 0.0 }],
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constants() {
        let tn = TerraNulliusImpl::new();
        assert_eq!(tn.small_id(), 0.0);
        assert_eq!(tn.client_id(), "TERRA_NULLIUS_CLIENT_ID");
        assert!(tn.id().is_none());
        assert!(!tn.is_player());
    }

    #[test]
    fn run_op_kinds() {
        assert_eq!(run_op(0, &[0.0]), vec![0.0]);
        let cid = run_op(1, &[1.0]);
        let units: Vec<u16> = CLIENT_ID.encode_utf16().collect();
        assert_eq!(cid[0] as usize, units.len());
        assert_eq!(&cid[1..], &units.iter().map(|&u| u as f64).collect::<Vec<_>>()[..]);
        assert_eq!(run_op(2, &[2.0]), vec![-1.0]);
        assert_eq!(run_op(3, &[3.0]), vec![0.0]);
        assert_eq!(run_op(9, &[]), Vec::<f64>::new());
    }
}
