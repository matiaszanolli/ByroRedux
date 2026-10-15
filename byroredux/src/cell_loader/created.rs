//! Created Object "At" references (M47.2) — the engine half of
//! `byroredux_scripting::created_references`.
//!
//! The alias refresh records a created reference (FormID, base, the reference
//! it is created at). Here each new record is *placed* at its target
//! reference's placement — the CK's "placed at the same location as another
//! Reference Alias (usually a marker)" — and every placed record whose cell is
//! loaded but whose entity is gone is *spawned* through the ordinary reference
//! loader. "Gone" covers both the first creation and a cell that unloaded
//! (which despawns everything under its `CellRoot`) and loaded again, so one
//! rule keeps a created reference present whenever its cell is.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use byroredux_core::ecs::components::GlobalTransform;
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::{CellRoot, Resource, World};
use byroredux_plugin::esm::cell::{CellRef, PlacedRef};
use byroredux_renderer::VulkanContext;
use byroredux_scripting::{CreatedCell, CreatedPlacement, CreatedReferences};

use crate::asset_provider::{MaterialProvider, TextureProvider};

/// The live entity of each spawned created reference, by FormID. Session
/// state: re-derived by spawning, so `NOT_SAVED_BY_DESIGN`.
#[derive(Debug, Default)]
pub(crate) struct CreatedReferenceEntities(pub HashMap<u32, EntityId>);

impl Resource for CreatedReferenceEntities {}

fn created_cell(cell: CellRef<'_>) -> CreatedCell {
    match cell {
        CellRef::Interior { editor_id } => CreatedCell::Interior {
            editor_id: editor_id.to_string(),
        },
        CellRef::Exterior { worldspace, grid } => CreatedCell::Exterior {
            worldspace: worldspace.to_string(),
            grid,
        },
    }
}

/// The player's cell and position, for a reference created at the player.
fn player_placement(world: &World) -> Option<CreatedPlacement> {
    let player = world
        .try_resource::<crate::systems::PlayerEntity>()
        .and_then(|player| player.0)?;
    let position_zup = byroredux_core::math::coord::yup_to_zup_pos(
        world.get::<GlobalTransform>(player)?.translation.to_array(),
    );
    let cell = if let Some(cell) = world.try_resource::<super::CurrentCellContext>() {
        CreatedCell::Interior {
            editor_id: cell.cell_editor_id.clone(),
        }
    } else {
        let exterior = world.try_resource::<super::CurrentExteriorContext>()?;
        let units = byroredux_core::math::coord::EXTERIOR_CELL_UNITS;
        CreatedCell::Exterior {
            worldspace: exterior.worldspace_key.clone(),
            grid: (
                (position_zup[0] / units).floor() as i32,
                (position_zup[1] / units).floor() as i32,
            ),
        }
    };
    Some(CreatedPlacement {
        cell,
        position_zup,
        rotation_zup: [0.0; 3],
    })
}

/// Place every record that has no placement yet, at its target reference's
/// placement (the player's position for the player; another created
/// reference's placement for one). Returns how many were placed.
pub(crate) fn place_created_references(world: &World) -> usize {
    let (unplaced, placed): (Vec<_>, HashMap<u32, CreatedPlacement>) = {
        let Some(created) = world.try_resource::<CreatedReferences>() else {
            return 0;
        };
        let unplaced: Vec<(u32, u32)> = created
            .iter()
            .filter(|record| record.placement.is_none())
            .map(|record| (record.form_id, record.at_reference))
            .collect();
        if unplaced.is_empty() {
            return 0;
        }
        let placed = created
            .iter()
            .filter_map(|record| Some((record.form_id, record.placement.clone()?)))
            .collect();
        (unplaced, placed)
    };
    let index = world
        .try_resource::<super::LoadedCellIndex>()
        .map(|index| Arc::clone(&index.0));
    let mut resolved = Vec::new();
    for (form_id, at) in unplaced {
        let placement = if at == byroredux_scripting::PLAYER_REFERENCE {
            player_placement(world)
        } else if let Some((cell, reference)) = index
            .as_ref()
            .and_then(|index| index.cells.locate_refr(at))
        {
            // A reference that is loaded now stands where it is now, not
            // where it was authored: a `PlaceAtMe` on a moved actor puts the
            // new reference beside the actor.
            let position_zup = byroredux_scripting::condition::resolve_entity_by_global_form_id(world, at)
                .and_then(|entity| world.get::<GlobalTransform>(entity))
                .map_or(reference.position, |transform| {
                    byroredux_core::math::coord::yup_to_zup_pos(transform.translation.to_array())
                });
            Some(CreatedPlacement {
                cell: created_cell(cell),
                position_zup,
                rotation_zup: reference.rotation,
            })
        } else {
            placed.get(&at).cloned()
        };
        match placement {
            Some(placement) => resolved.push((form_id, placement)),
            None => log::debug!(
                "created reference {form_id:08X}: its target {at:08X} has no placement yet"
            ),
        }
    }
    let count = resolved.len();
    let mut created = world.resource_mut::<CreatedReferences>();
    for (form_id, placement) in resolved {
        created.place(form_id, placement);
    }
    count
}

/// The placed records in `cell` whose entity is not alive. A record its
/// script asked to `DeleteWhenAble` is deleted here instead: its entity is
/// gone, so its cell detached (M47.2).
pub(crate) fn unspawned_in(world: &World, cell: &CreatedCell) -> Vec<PlacedRef> {
    let mut deleting: Vec<u32> = Vec::new();
    let wanted: Vec<PlacedRef> = {
        let Some(created) = world.try_resource::<CreatedReferences>() else {
            return Vec::new();
        };
        created
            .iter()
            .filter(|record| record.deletion != byroredux_scripting::CreatedDeletion::Deleted)
            .filter_map(|record| {
                if record.deletion == byroredux_scripting::CreatedDeletion::WhenAble {
                    deleting.push(record.form_id);
                }
                let placement = record.placement.as_ref()?;
                let same_cell = match (&placement.cell, cell) {
                    (
                        CreatedCell::Interior { editor_id: a },
                        CreatedCell::Interior { editor_id: b },
                    ) => a.eq_ignore_ascii_case(b),
                    (a, b) => a == b,
                };
                same_cell.then(|| {
                    PlacedRef::at(
                        record.form_id,
                        record.base,
                        placement.position_zup,
                        placement.rotation_zup,
                    )
                })
            })
            .collect()
    };
    if wanted.is_empty() && deleting.is_empty() {
        return wanted;
    }
    let live: HashSet<u32> = world
        .try_resource::<CreatedReferenceEntities>()
        .map(|entities| {
            entities
                .0
                .iter()
                .filter(|(_, entity)| world.has::<CellRoot>(**entity))
                .map(|(form_id, _)| *form_id)
                .collect()
        })
        .unwrap_or_default();
    let deleted: HashSet<u32> = deleting
        .into_iter()
        .filter(|form_id| !live.contains(form_id))
        .collect();
    if !deleted.is_empty() {
        if let Some(mut created) = world.try_resource_mut::<CreatedReferences>() {
            for &form_id in &deleted {
                created.mark_deleted(form_id);
                log::info!("Deleted created reference {form_id:08X} (DeleteWhenAble)");
            }
        }
    }
    wanted
        .into_iter()
        .filter(|placed| !live.contains(&placed.form_id) && !deleted.contains(&placed.form_id))
        .collect()
}

/// Spawn created references into the loaded cell `cell_root` through the
/// ordinary reference loader, and record their entities.
pub(crate) fn spawn_created_references(
    world: &mut World,
    ctx: &mut VulkanContext,
    tex_provider: &TextureProvider,
    mat_provider: Option<&mut MaterialProvider>,
    cell_root: EntityId,
    refs: &[PlacedRef],
) {
    let Some(index) = world
        .try_resource::<super::LoadedCellIndex>()
        .map(|index| Arc::clone(&index.0))
    else {
        return;
    };
    let load_order = world
        .try_resource::<super::load_order::ActiveLoadOrder>()
        .map(|order| Arc::clone(&order.0))
        .unwrap_or_default();
    let first = world.next_entity_id();
    super::references::load_references(
        refs,
        &index.cells,
        &index,
        &index.races,
        index.game,
        world,
        ctx,
        tex_provider,
        mat_provider,
        "created references",
        &load_order,
        &HashSet::new(),
    );
    let last = world.next_entity_id();
    super::load::stamp_cell_root_range(world, cell_root, first, last);
    let spawned: Vec<(u32, EntityId)> = world
        .query::<byroredux_scripting::SceneAliasCandidate>()
        .map(|candidates| {
            (first..last)
                .filter_map(|entity| {
                    let candidate = candidates.get(entity)?;
                    refs.iter()
                        .any(|placed| placed.form_id == candidate.reference_form_id)
                        .then_some((candidate.reference_form_id, entity))
                })
                .collect()
        })
        .unwrap_or_default();
    for (form_id, _) in &spawned {
        log::info!("Spawned created reference {form_id:08X}");
    }
    world.init_resource::<CreatedReferenceEntities>();
    let mut entities = world.resource_mut::<CreatedReferenceEntities>();
    entities.0.extend(spawned);
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_plugin::esm::cell::CellData;
    use byroredux_scripting::QuestFormId;

    /// A record is placed at its target's placement, and only a placed record
    /// of the asked cell without a live entity wants spawning.
    #[test]
    fn records_place_at_their_target_and_respawn_only_when_gone() {
        let mut world = World::new();
        let mut index = byroredux_plugin::esm::records::EsmIndex::default();
        index.cells.cells.insert(
            "dunhold".into(),
            CellData {
                form_id: 0x10,
                editor_id: "DunHold".into(),
                references: vec![PlacedRef::at(0x55, 0x32, [1.0, 2.0, 3.0], [0.0, 0.0, 0.5])],
                ..Default::default()
            },
        );
        world.insert_resource(super::super::LoadedCellIndex(Arc::new(index)));
        let mut created = CreatedReferences::default();
        let form_id = created.create(QuestFormId(7), 1, 0xB1, 0, 0x55).unwrap();
        let lost = created.create(QuestFormId(7), 2, 0xB2, 0, 0x99).unwrap();
        world.insert_resource(created);

        assert_eq!(place_created_references(&world), 1, "0x99 is nowhere");
        let placement = world
            .resource::<CreatedReferences>()
            .get(QuestFormId(7), 1)
            .and_then(|record| record.placement.clone())
            .unwrap();
        assert_eq!(placement.position_zup, [1.0, 2.0, 3.0]);
        assert_eq!(placement.rotation_zup, [0.0, 0.0, 0.5]);
        assert!(world
            .resource::<CreatedReferences>()
            .iter()
            .any(|record| record.form_id == lost && record.placement.is_none()));

        let here = CreatedCell::Interior {
            editor_id: "DUNHOLD".into(),
        };
        let wanted = unspawned_in(&world, &here);
        assert_eq!(wanted.len(), 1);
        assert_eq!((wanted[0].form_id, wanted[0].base_form_id), (form_id, 0xB1));
        let elsewhere = CreatedCell::Interior {
            editor_id: "Other".into(),
        };
        assert!(unspawned_in(&world, &elsewhere).is_empty());

        // Spawned and alive: not wanted. Its cell unloaded (no `CellRoot`):
        // wanted again.
        let entity = world.spawn();
        world.insert(entity, CellRoot(entity));
        world.insert_resource(CreatedReferenceEntities(HashMap::from([(form_id, entity)])));
        assert!(unspawned_in(&world, &here).is_empty());
        world.despawn(entity);
        assert_eq!(unspawned_in(&world, &here).len(), 1);

        // M47.2 — DeleteWhenAble while alive: kept until its cell is gone,
        // then deleted for good — no respawn, and the tombstone stays so the
        // alias refresh does not create it again.
        let entity = world.spawn();
        world.insert(entity, CellRoot(entity));
        world.insert_resource(CreatedReferenceEntities(HashMap::from([(form_id, entity)])));
        assert!(world.resource_mut::<CreatedReferences>().delete_when_able(form_id));
        assert!(unspawned_in(&world, &here).is_empty());
        assert_eq!(
            world.resource::<CreatedReferences>().get(QuestFormId(7), 1).unwrap().deletion,
            byroredux_scripting::CreatedDeletion::WhenAble,
            "alive: not yet"
        );
        world.despawn(entity);
        assert!(unspawned_in(&world, &here).is_empty(), "deleted, not respawned");
        assert_eq!(
            world.resource::<CreatedReferences>().get(QuestFormId(7), 1).unwrap().deletion,
            byroredux_scripting::CreatedDeletion::Deleted
        );
        assert!(unspawned_in(&world, &here).is_empty());
        assert!(
            !world.resource_mut::<CreatedReferences>().delete_when_able(0x0001_2345),
            "an authored reference cannot be deleted"
        );
    }
}
