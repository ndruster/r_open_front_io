//! Port of `src/core/game/TransportShipUtils.ts`.
//!
//! Three pure helpers over a [`SpatialQuery`]. The player/game facets they
//! read are passed in as values — the capture drives them from the mock `Game`
//! facade the same way (`unitCount`, `boatMaxNumber`, `owner(tile)`,
//! `canAttackPlayer`, border tiles, tileChangeVersion), so the parity replay
//! supplies scripted answers bit-identically.
//!
//! Faithfulness notes:
//!
//! * The owner-same check is TS `other === player` (object identity); the
//!   harness models it as equal smallIDs (smallIDs are unique per player).
//! * `targetTransportTile` runs *before* the owner/attackability checks, so
//!   its `reachableComponents` memo refresh happens even when the answer is
//!   later discarded — order preserved here.
//! * `closestShoreByWater` returning `null` for a terra nullius attacker is
//!   modelled by the caller passing an empty border (TS `borderTiles()` of
//!   terra nullius is empty → `shores.length === 0` → `null`).

use crate::spatial_query::SpatialQuery;

/// `canBuildTransportShip(game, player, tile)` — returns the shore tile to
/// launch from, or `None`.
///
/// * `transport_count` — `player.unitCount(UnitType.TransportShip)`
/// * `boat_max` — `game.config().boatMaxNumber()`
/// * `owner_small_id` / `owner_is_player` — `game.owner(tile)` facets
/// * `self_small_id` — the attacker's smallID (identity check)
/// * `can_attack` — `player.canAttackPlayer(owner)` when the owner is a player
/// * `attacker_border` / `attacker_tile_version` — `player.borderTiles()` /
///   `player.tileChangeVersion()` feeding the reachable-components memo
#[allow(clippy::too_many_arguments)] // TS game/player args decompose into faceted values
pub fn can_build_transport_ship(
    sp: &mut SpatialQuery,
    transport_count: f64,
    boat_max: f64,
    tile: f64,
    owner_small_id: f64,
    owner_is_player: bool,
    self_small_id: f64,
    can_attack: bool,
    attacker_border: &[f64],
    attacker_tile_version: f64,
) -> Option<f64> {
    if transport_count >= boat_max {
        return None;
    }

    let dst = target_transport_tile(
        sp,
        owner_small_id,
        attacker_border,
        attacker_tile_version,
        tile,
    )?;

    // `other === player`.
    if owner_is_player && owner_small_id == self_small_id {
        return None;
    }
    if owner_is_player && !can_attack {
        return None;
    }

    sp.closest_shore_by_water(attacker_border, dst)
}

/// `targetTransportTile(gm, attacker, tile)` — the closest shore owned by the
/// tile's owner that the attacker can actually reach by water (a shore facing
/// a disconnected inland lake is never chosen).
pub fn target_transport_tile(
    sp: &mut SpatialQuery,
    owner_small_id: f64,
    attacker_border: &[f64],
    attacker_tile_version: f64,
    tile: f64,
) -> Option<f64> {
    sp.closest_reachable_shore(owner_small_id, attacker_border, attacker_tile_version, tile, 50.0)
}

/// `bestShoreDeploymentSource(gm, player, dst)` — the attacker's shore on the
/// water component containing `dst`, refined by the bounded A* pass.
pub fn best_shore_deployment_source(
    sp: &mut SpatialQuery,
    attacker_border: &[f64],
    attacker_tile_version: f64,
    dst: f64,
) -> Option<f64> {
    let _ = attacker_tile_version;
    sp.closest_shore_by_water(attacker_border, dst)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_map::GameMap;

    fn maps() -> (GameMap, GameMap) {
        // 8x8: land rows 0-2, water rows 3-6, land row 7.
        let mut terrain = vec![0x00u8; 64];
        terrain[..24].fill(0x83);
        terrain[56..64].fill(0x83);
        (
            GameMap::new(8.0, 8.0, terrain, 24.0),
            GameMap::new(4.0, 4.0, vec![0x83; 16], 8.0),
        )
    }

    #[test]
    fn boat_max_gate() {
        let (map, mini) = maps();
        let mut sp = SpatialQuery::new(map, mini, true);
        assert_eq!(
            can_build_transport_ship(&mut sp, 5.0, 5.0, 28.0, 2.0, true, 1.0, true, &[20.0], 0.0),
            None
        );
    }

    #[test]
    fn same_owner_gate() {
        let (mut map, mini) = maps();
        // Attacker owns the target tile's shore: tile 20 is owner 1's own.
        map.set_owner_id(20.0, 1.0);
        let mut sp = SpatialQuery::new(map, mini, true);
        assert_eq!(
            can_build_transport_ship(&mut sp, 0.0, 5.0, 28.0, 1.0, true, 1.0, true, &[20.0], 0.0),
            None
        );
    }

    #[test]
    fn cannot_attack_gate() {
        let (mut map, mini) = maps();
        map.set_owner_id(20.0, 2.0);
        let mut sp = SpatialQuery::new(map, mini, true);
        assert_eq!(
            can_build_transport_ship(&mut sp, 0.0, 5.0, 28.0, 2.0, true, 1.0, false, &[60.0], 0.0),
            None
        );
    }
}
