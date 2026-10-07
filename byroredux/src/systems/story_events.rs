//! Engine-side Story Manager event producers — #5366 Phase 1.
//!
//! The scripting crate owns dispatch (`story_manager_dispatch_system`)
//! and the marker; this module owns the engine surfaces that raise it.
//! Phase 1 wires `CLOC` here (the session's location identity from the
//! cell-loader contexts) — `KILL` is raised directly at the combat
//! death site in `crate::combat`, which has the aggressor in scope.

use crate::cell_loader::{CurrentCellContext, CurrentExteriorContext};
use crate::systems::character::PlayerEntity;
use byroredux_core::ecs::world::World;
use std::hash::{Hash, Hasher};

/// System: raise a `CLOC` story event when the session's location
/// identity changes. Interior identity is the cell editor-id; exterior
/// identity is worldspace + grid, so every grid crossing fires — a
/// coarser cadence than Skyrim's LCTN granularity, tightened in #5366
/// Phase 2 when event-carried location FormIDs map through LCTN.
///
/// No player entity (loose-NIF demo, spawn ladders): no event — a
/// location change nobody occupies is not a story event.
pub fn story_change_location_system(world: &World) {
    let Some(player) = world
        .try_resource::<PlayerEntity>()
        .and_then(|player| player.0)
    else {
        return;
    };
    let key = if let Some(cell) = world.try_resource::<CurrentCellContext>() {
        let parts: [&str; 2] = [&cell.esm_path, &cell.cell_editor_id];
        Some(location_hash(&parts))
    } else if let Some(exterior) = world.try_resource::<CurrentExteriorContext>() {
        let grid = format!("grid:{:?}", exterior.grid);
        let parts: [&str; 3] = [&exterior.esm_path, &exterior.worldspace_key, &grid];
        Some(location_hash(&parts))
    } else {
        // Neither context: mid-transition window or loose-NIF mode. The
        // cursor follows so the post-transition frame counts as a change.
        None
    };
    byroredux_scripting::story_manager::emit_change_location_on_key_change(
        world, key, player, None,
    );
}

/// Process-local identity hash for change detection only — never
/// persisted, so `DefaultHasher`'s unspecified seeding is fine.
fn location_hash(parts: &[&str]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for part in parts {
        part.hash(&mut hasher);
    }
    hasher.finish()
}
