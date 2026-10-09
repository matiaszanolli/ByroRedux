//! Engine-side Story Manager event producers — #5366 Phases 1–2.
//!
//! The scripting crate owns dispatch (`story_manager_dispatch_system`)
//! and the marker; this module owns the engine surfaces that raise it.
//! Phase 1 wires `CLOC` here (the session's location identity from the
//! cell-loader contexts) — `KILL` is raised directly at the combat
//! death site in `crate::combat`, which has the aggressor in scope.
//!
//! Phase 2 tightens the CLOC key to Skyrim's location granularity: when
//! the current cell resolves an LCTN (`XLCN`, already decoded into
//! `CellData::location_form`), the key is that LCTN — moving between
//! two cells of one location no longer fires, moving within one hold
//! between authored locations does. Cells without an LCTN (wilderness
//! grids) keep the Phase-1 worldspace+grid key, so cadence there is
//! unchanged. The resolved LCTN rides on the event as `L2` (new) with
//! the previous fire's as `L1` (old).

use crate::cell_loader::{CurrentCellContext, CurrentExteriorContext, LoadedCellIndex};
use crate::systems::character::PlayerEntity;
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::world::World;
use std::hash::{Hash, Hasher};

/// System: raise a `CLOC` story event when the session's location
/// identity changes.
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
    // LCTN first (Phase 2): interior cells by editor-id, exterior grid
    // cells by worldspace+grid. The loaded index is installed on every
    // load path; without it (or without an `XLCN`) the location stays
    // `None` and the key falls back to the Phase-1 identity.
    let location = resolve_current_lctn(world);
    let key = if let Some(lctn) = location {
        let parts: [&str; 2] = ["lctn", &format!("{:08X}", lctn)];
        Some(location_hash(&parts))
    } else if let Some(cell) = world.try_resource::<CurrentCellContext>() {
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
        world, key, player, location,
    );
}

/// The current cell's `XLCN` LCTN, interior or exterior grid cell, from
/// the loaded plugin index. `None` when the index is absent, the cell
/// isn't found, or the record authors no location (wilderness grids).
pub(crate) fn resolve_current_lctn(world: &World) -> Option<u32> {
    let index = world.try_resource::<LoadedCellIndex>()?.0.clone();
    if let Some(cell) = world.try_resource::<CurrentCellContext>() {
        return index
            .cells
            .cells
            .get(&cell.cell_editor_id.to_ascii_lowercase())
            .and_then(|data| data.location_form);
    }
    if let Some(exterior) = world.try_resource::<CurrentExteriorContext>() {
        return index
            .cells
            .exterior_cells
            .get(&exterior.worldspace_key)
            .and_then(|grid| grid.get(&exterior.grid))
            .and_then(|data| data.location_form);
    }
    None
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

/// #5366 Phase 4 — `AHEL` (Actor Hello): raised when a conversation
/// opens (activation greeting or force-greet — the #5367 surfaces).
/// `OnStoryHello(akLocation, akActor1, akActor2)`: L1 = the session's
/// current LCTN, R1 = the greeter, R2 = the greeted party. On
/// pre-Creation titles this is a no-op (no SM tree installed).
pub(crate) fn raise_hello_story_event(world: &World, greeter: EntityId, greeted: EntityId) {
    use byroredux_scripting::story_manager::StoryEvent;
    // #5372 — resolve the location BEFORE taking the StoryEvent write
    // guard: `resolve_current_lctn` reads `LoadedCellIndex`, and every
    // dialogue-open call site (forcegreet_open, the selection system's
    // Pass 2) already holds a `LoadedCellIndex` read of its own. As a
    // struct-field expression it evaluated under the marker guard and
    // closed the cycle #5066 predicted (LoadedCellIndex -> StoryEvent
    // -> LoadedCellIndex).
    let location_1 = resolve_current_lctn(world);
    let Some(mut events) = world.query_mut::<StoryEvent>() else {
        return;
    };
    events.insert(
        greeter,
        StoryEvent {
            mnemonic: *b"AHEL",
            reference_1: greeter,
            reference_2: Some(greeted),
            location_1,
            location_2: None,
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #5366 Phase 4 — the AHEL producer stamps the hello shape the CK
    /// documents (`OnStoryHello(akLocation, akActor1, akActor2)`):
    /// greeter → R1, greeted → R2, session LCTN → L1 (absent index →
    /// none, the loose-NIF/no-context shape).
    #[test]
    fn hello_event_carries_greeter_greeted_and_session_location() {
        let mut world = World::new();
        byroredux_scripting::register(&mut world);
        let greeter = world.spawn();
        let greeted = world.spawn();
        raise_hello_story_event(&world, greeter, greeted);
        let event = world
            .query::<byroredux_scripting::story_manager::StoryEvent>()
            .map(|events| events.iter().next().map(|(_, event)| *event))
            .unwrap_or_default()
            .expect("the hello marker is raised on the greeter");
        assert_eq!(event.mnemonic, *b"AHEL");
        assert_eq!(event.reference_1, greeter);
        assert_eq!(event.reference_2, Some(greeted));
        assert_eq!(event.location_1, None, "no loaded index — no session LCTN");
        assert_eq!(event.location_2, None);
    }
}
