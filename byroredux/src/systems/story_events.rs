//! Engine-side Story Manager event producers — #5366 Phases 1–2.
//!
//! The scripting crate owns dispatch (`story_manager_dispatch_system`)
//! and the marker; this module owns the engine surfaces that raise it.
//! Phase 1 wires `CLOC` here (the session's location identity from the
//! cell-loader contexts) — `KILL` is raised by [`raise_kill_story_event`] at
//! each live death producer (combat carries the aggressor; drowning and
//! the SDK batch have none).
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
use byroredux_scripting::SmTree;
use std::hash::{Hash, Hasher};

/// The kind of cell the player occupies (`IsInInterior` / `GetInWorldspace`): interior while an
/// interior context is installed, exterior (with the worldspace's form id) while an exterior one
/// is, none in loose-NIF mode or mid-transition.
fn player_cell_kind(world: &World) -> Option<byroredux_scripting::cell_kind::CellKind> {
    use byroredux_scripting::cell_kind::CellKind;
    if world.try_resource::<CurrentCellContext>().is_some() {
        Some(CellKind::Interior)
    } else if let Some(exterior) = world.try_resource::<CurrentExteriorContext>() {
        world
            .try_resource::<LoadedCellIndex>()
            .and_then(|index| index.0.cells.worldspaces.get(&exterior.worldspace_key).map(|record| record.form_id))
            .map(|worldspace| CellKind::Exterior { worldspace })
    } else {
        None
    }
}

/// What the per-frame system derives from the session's cell contexts: the kind of cell, its LCTN
/// and the CLOC change key.
#[derive(Clone, Copy, Default)]
struct DerivedLocation {
    kind: Option<byroredux_scripting::cell_kind::CellKind>,
    location: Option<u32>,
    key: Option<u64>,
}

/// The last derivation and the identity of the contexts it came from (#6335).
/// `NOT_SAVED_BY_DESIGN`: re-derived on the first frame after a load.
#[derive(Default)]
pub(crate) struct StoryLocationMemo {
    identity: Option<u64>,
    derived: DerivedLocation,
}

impl byroredux_core::ecs::resource::Resource for StoryLocationMemo {}

/// A hash of everything [`derive_location`] reads, built without allocating: the loaded index (by
/// address), and which cell context is installed with its identity fields. The contexts and the
/// index are replaced wholesale on a transition or a content load, so this moves exactly then.
fn context_identity(world: &World) -> u64 {
    let mut hasher = rustc_hash::FxHasher::default();
    world
        .try_resource::<LoadedCellIndex>()
        .map_or(0usize, |index| std::sync::Arc::as_ptr(&index.0) as usize)
        .hash(&mut hasher);
    if let Some(cell) = world.try_resource::<CurrentCellContext>() {
        1u8.hash(&mut hasher);
        cell.esm_path.hash(&mut hasher);
        cell.cell_editor_id.hash(&mut hasher);
    } else if let Some(exterior) = world.try_resource::<CurrentExteriorContext>() {
        2u8.hash(&mut hasher);
        exterior.esm_path.hash(&mut hasher);
        exterior.worldspace_key.hash(&mut hasher);
        exterior.grid.hash(&mut hasher);
    } else {
        3u8.hash(&mut hasher);
    }
    hasher.finish()
}

/// The cell kind, LCTN and CLOC key for the session's current cell — from the memo while the
/// contexts are unchanged, otherwise derived from the string-keyed maps (an interior lowercases its
/// editor id into a fresh `String`) and remembered. The result changes only on a cell transition or
/// an exterior grid step, so deriving it every frame was two allocations and several string-hash
/// lookups for nothing (#6335). Takes no guard across the derivation: `resolve_current_lctn` reads
/// `LoadedCellIndex` (#5372).
fn derive_location(world: &World) -> DerivedLocation {
    let identity = context_identity(world);
    if let Some(memo) = world.try_resource::<StoryLocationMemo>() {
        if memo.identity == Some(identity) {
            return memo.derived;
        }
    }
    let location = resolve_current_lctn(world);
    let derived = DerivedLocation { kind: player_cell_kind(world), location, key: location_key(world, location) };
    if let Some(mut memo) = world.try_resource_mut::<StoryLocationMemo>() {
        memo.identity = Some(identity);
        memo.derived = derived;
    }
    derived
}

/// System: raise a `CLOC` story event when the session's location
/// identity changes.
///
/// No player entity (loose-NIF demo, spawn ladders): no event — a
/// location change nobody occupies is not a story event.
pub fn story_change_location_system(world: &World) {
    let derived = derive_location(world);
    // Publish the kind of cell the player occupies; only written when it changes.
    if let Some(mut published) = world.try_resource_mut::<byroredux_scripting::cell_kind::PlayerCellKind>() {
        if published.0 != derived.kind {
            published.0 = derived.kind;
        }
    }
    // M47.2 — publish the player's current location for the location
    // conditions (the player is not cell-owned, so it has no `CellRoot` to
    // follow). Only where the load order authors locations: FO3/FNV/Oblivion
    // have none and keep #5447's no-per-frame-work contract.
    if world
        .try_resource::<byroredux_scripting::LocationTable>()
        .is_some_and(|table| !table.is_empty())
    {
        if let Some(mut published) =
            world.try_resource_mut::<byroredux_scripting::PlayerLocation>()
        {
            if published.0 != derived.location {
                published.0 = derived.location;
            }
        }
    }
    // #5447 — a load order that authors no Story Manager nodes (FO3, FNV,
    // Oblivion; `install_story_manager` still installs an empty tree and
    // cursor on them) has nothing to dispatch a CLOC to, so skip the key
    // derivation instead of paying it every frame. No tree at all is the
    // same case: the cursor is installed alongside it.
    if world
        .try_resource::<SmTree>()
        .is_none_or(|tree| tree.nodes.is_empty())
    {
        return;
    }
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
    byroredux_scripting::story_manager::emit_change_location_on_key_change(
        world, derived.key, player, derived.location,
    );
}

/// The session's location identity for CLOC change detection: the LCTN
/// when the current cell resolves one, else the interior cell / exterior
/// worldspace+grid identity. `None` mid-transition and in loose-NIF mode
/// (neither context installed). Hashed from the typed fields directly —
/// no per-frame `String` building (#5447).
fn location_key(world: &World, location: Option<u32>) -> Option<u64> {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    // The leading tag keeps the three identity kinds from colliding.
    if let Some(lctn) = location {
        0u8.hash(&mut hasher);
        lctn.hash(&mut hasher);
    } else if let Some(cell) = world.try_resource::<CurrentCellContext>() {
        1u8.hash(&mut hasher);
        cell.esm_path.hash(&mut hasher);
        cell.cell_editor_id.hash(&mut hasher);
    } else if let Some(exterior) = world.try_resource::<CurrentExteriorContext>() {
        2u8.hash(&mut hasher);
        exterior.esm_path.hash(&mut hasher);
        exterior.worldspace_key.hash(&mut hasher);
        exterior.grid.hash(&mut hasher);
    } else {
        // Neither context: mid-transition window or loose-NIF mode. The
        // cursor follows so the post-transition frame counts as a change.
        return None;
    }
    // Process-local identity hash for change detection only — never
    // persisted, so `DefaultHasher`'s unspecified seeding is fine.
    Some(hasher.finish())
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
    byroredux_scripting::story_manager::raise_story_event(
        world,
        greeter,
        StoryEvent {
            mnemonic: *b"AHEL",
            reference_1: greeter,
            reference_2: Some(greeted),
            location_1,
            location_2: None,
            scripted: None,
        },
    );
}

/// M47.2 — the victim's Papyrus `OnDeath(akKiller)`.
pub(crate) fn raise_death_event(world: &World, victim: EntityId, killer: Option<EntityId>) {
    byroredux_scripting::obscript_state::note_death(world, victim, killer);
    if let Some(mut deaths) = world.query_mut::<byroredux_scripting::OnDeathEvent>() {
        deaths.insert(victim, byroredux_scripting::OnDeathEvent { killer });
    }
}

/// #5434 — `KILL` (Actor Kill), raised from every LIVE death producer
/// (combat damage, NPC/player drowning, the SDK actor-value batch) — never
/// from corpse restores, which re-assert a death that already fired.
/// Slots: R1 = victim, R2 = killer (`None` for environmental deaths such as
/// drowning), L1 = the session's current LCTN (`WIKill06` authors a
/// KILL→L1 alias fill). On pre-Creation titles this is a no-op (no SM tree
/// installed).
pub(crate) fn raise_kill_story_event(world: &World, victim: EntityId, killer: Option<EntityId>) {
    use byroredux_scripting::story_manager::StoryEvent;
    // #5372 — resolve the location BEFORE taking the StoryEvent write
    // guard (`resolve_current_lctn` reads `LoadedCellIndex`).
    let location_1 = resolve_current_lctn(world);
    raise_death_event(world, victim, killer);
    byroredux_scripting::story_manager::raise_story_event(
        world,
        victim,
        StoryEvent {
            mnemonic: *b"KILL",
            reference_1: victim,
            reference_2: killer,
            location_1,
            location_2: None,
            scripted: None,
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

    /// #5434 — a world whose session sits in a cell that authors LCTN
    /// `0x0001_0A0B`.
    fn world_in_located_cell() -> World {
        use byroredux_plugin::esm::cell::CellData;
        let mut world = World::new();
        byroredux_scripting::register(&mut world);
        let cell = CellData {
            form_id: 0x0001_2345,
            editor_id: "WhiterunBanneredMare".to_string(),
            is_interior: true,
            location_form: Some(0x0001_0A0B),
            ..Default::default()
        };
        let mut loaded = byroredux_plugin::esm::records::EsmIndex::default();
        loaded.cells.cells.insert("whiterunbanneredmare".to_string(), cell);
        world.insert_resource(LoadedCellIndex(std::sync::Arc::new(loaded)));
        world.insert_resource(CurrentCellContext {
            cell_editor_id: "WhiterunBanneredMare".to_string(),
            esm_path: String::new(),
            masters: Vec::new(),
        });
        world
    }

    /// A world in an interior cell with a player, the CLOC cursor, and a
    /// Story Manager tree of `node_count` event roots (#5447).
    fn world_with_sm_tree(node_count: u32) -> (World, EntityId) {
        use byroredux_plugin::esm::records::{SmNodeKind, SmNodeRecord};
        let mut world = World::new();
        byroredux_scripting::register(&mut world);
        let records = (0..node_count)
            .map(|i| {
                let form_id = 0x100 + i;
                (
                    form_id,
                    SmNodeRecord {
                        form_id,
                        kind: SmNodeKind::Event,
                        event_mnemonic: Some(*b"CLOC"),
                        ..Default::default()
                    },
                )
            })
            .collect();
        byroredux_scripting::story_manager::install_story_manager(&mut world, &records);
        let player = world.spawn();
        world.insert_resource(PlayerEntity(Some(player)));
        world.insert_resource(CurrentCellContext {
            cell_editor_id: "WhiterunBanneredMare".to_string(),
            esm_path: String::new(),
            masters: Vec::new(),
        });
        (world, player)
    }

    /// #6335 — the cell kind, LCTN and CLOC key come from the memo while the contexts are
    /// unchanged and are re-derived when they move: a poisoned memo is served on a hit (proving the
    /// string maps were not read), and replaced after the cell changes.
    #[test]
    fn the_location_is_derived_once_per_context_not_per_frame() {
        use byroredux_scripting::cell_kind::{CellKind, PlayerCellKind};
        let (mut world, _player) = world_with_sm_tree(1);
        let mut index = byroredux_plugin::esm::records::EsmIndex::default();
        for (name, lctn) in [("whiterunbanneredmare", 0x0001_0A0B), ("dragonsreach", 0x0001_0A0C)] {
            index.cells.cells.insert(
                name.to_string(),
                byroredux_plugin::esm::cell::CellData { location_form: Some(lctn), is_interior: true, ..Default::default() },
            );
        }
        world.insert_resource(LoadedCellIndex(std::sync::Arc::new(index)));
        world.insert_resource(StoryLocationMemo::default());
        world.insert_resource(PlayerCellKind::default());

        story_change_location_system(&world);
        {
            let memo = world.resource::<StoryLocationMemo>();
            assert_eq!(memo.derived.location, Some(0x0001_0A0B));
            assert_eq!(memo.derived.kind, Some(CellKind::Interior));
        }

        let poisoned = Some(CellKind::Exterior { worldspace: 7 });
        world.resource_mut::<StoryLocationMemo>().derived.kind = poisoned;
        story_change_location_system(&world);
        assert_eq!(world.resource::<PlayerCellKind>().0, poisoned, "an unchanged context is served from the memo");

        world.resource_mut::<CurrentCellContext>().cell_editor_id = "DragonsReach".to_string();
        story_change_location_system(&world);
        assert_eq!(world.resource::<PlayerCellKind>().0, Some(CellKind::Interior), "a new cell is derived afresh");
        assert_eq!(world.resource::<StoryLocationMemo>().derived.location, Some(0x0001_0A0C));
    }

    /// #5447 — a load order with no Story Manager nodes (FO3 / FNV /
    /// Oblivion) never derives the CLOC key: the cursor stays untouched
    /// and no marker is raised, so the per-frame system is a pure early-out.
    #[test]
    fn change_location_system_is_inert_without_story_manager_nodes() {
        let (world, player) = world_with_sm_tree(0);
        story_change_location_system(&world);
        let cursor = world
            .try_resource::<byroredux_scripting::StoryLocationCursor>()
            .expect("install_story_manager creates the cursor");
        assert_eq!(cursor.key, None, "the key must not be derived");
        drop(cursor);
        assert!(world
            .query::<byroredux_scripting::StoryEvent>()
            .is_none_or(|events| events.get(player).is_none()));
    }

    /// #5447 — the same system with a populated tree still raises CLOC on
    /// the first frame and goes quiet while the location is unchanged.
    #[test]
    fn change_location_system_raises_cloc_once_with_story_manager_nodes() {
        let (mut world, player) = world_with_sm_tree(1);
        story_change_location_system(&world);
        let event = *world
            .query::<byroredux_scripting::StoryEvent>()
            .expect("StoryEvent storage")
            .get(player)
            .expect("the first frame raises CLOC");
        assert_eq!(event.mnemonic, *b"CLOC");
        world.remove::<byroredux_scripting::StoryEvent>(player);
        story_change_location_system(&world);
        assert!(
            world
                .query::<byroredux_scripting::StoryEvent>()
                .is_none_or(|events| events.get(player).is_none()),
            "an unchanged location raises nothing"
        );
    }

    /// #5434 — KILL carries victim → R1, killer → R2 and the session LCTN
    /// → L1, the same location the AHEL producer resolves (`WIKill06`
    /// authors a KILL→L1 alias fill).
    #[test]
    fn kill_event_carries_victim_killer_and_session_location() {
        let mut world = world_in_located_cell();
        let victim = world.spawn();
        let killer = world.spawn();
        raise_kill_story_event(&world, victim, Some(killer));
        let event = *world
            .query::<byroredux_scripting::story_manager::StoryEvent>()
            .expect("StoryEvent storage")
            .get(victim)
            .expect("the KILL marker is raised on the victim");
        assert_eq!(event.mnemonic, *b"KILL");
        assert_eq!(event.reference_1, victim);
        assert_eq!(event.reference_2, Some(killer));
        assert_eq!(event.location_1, Some(0x0001_0A0B));
        assert_eq!(event.location_2, None);
    }

    /// #5434 — an environmental death (drowning, SDK batch) has no killer.
    #[test]
    fn kill_event_without_a_killer_leaves_r2_empty() {
        let mut world = world_in_located_cell();
        let victim = world.spawn();
        raise_kill_story_event(&world, victim, None);
        let event = *world
            .query::<byroredux_scripting::story_manager::StoryEvent>()
            .expect("StoryEvent storage")
            .get(victim)
            .expect("the KILL marker is raised on the victim");
        assert_eq!(event.reference_2, None);
        assert_eq!(event.location_1, Some(0x0001_0A0B));
    }
}
