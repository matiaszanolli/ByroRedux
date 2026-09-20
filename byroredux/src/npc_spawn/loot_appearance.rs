//! Deferred appearance restoration for the current corpse take-all action.
//! Skeletons, physics and gameplay state stay in place. New body parts remain
//! hidden until the whole replacement is ready; superseded meshes are retained
//! until normal cell teardown, which owns their GPU/skin-slot lifetimes.

use super::*;
use byroredux_core::ecs::components::Dead;
use byroredux_core::ecs::{CellRoot, Children, Component, MeshHandle, SparseSetStorage};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

#[derive(Clone, Debug)]
pub(super) struct RestorePart {
    pub path: String,
    pub tint: Option<String>,
}

impl RestorePart {
    pub(super) fn body(path: &str) -> Self {
        Self {
            path: path.to_owned(),
            tint: None,
        }
    }
}

#[derive(Default)]
pub(crate) struct NpcLootAppearance {
    pub(super) skeleton: HashMap<Arc<str>, EntityId>,
    pub(super) parts: Vec<RestorePart>,
    pub(super) original_roots: Vec<EntityId>,
    staged_roots: Vec<EntityId>,
    next_part: usize,
    finished: bool,
    failed: bool,
}

impl Component for NpcLootAppearance {
    type Storage = SparseSetStorage<Self>;
}

/// Separate from animation visibility: a NIF controller must not reveal a
/// staged body part or superseded outfit. Applied only to mesh entities.
pub(crate) struct NpcAppearanceHidden;
impl Component for NpcAppearanceHidden {
    type Storage = SparseSetStorage<Self>;
}

pub(super) fn install(
    world: &mut World,
    actor: EntityId,
    mut appearance: NpcLootAppearance,
    skeleton: &HashMap<Arc<str>, EntityId>,
) {
    if appearance.original_roots.is_empty() {
        return;
    }
    appearance.skeleton.clone_from(skeleton);
    world.insert(actor, appearance);
}

fn fully_looted(world: &World, actor: EntityId) -> bool {
    if world.get::<Dead>(actor).is_none() {
        return false;
    }
    let empty = world
        .get::<Inventory>(actor)
        .is_some_and(|inventory| inventory.items.iter().all(|item| item.count == 0));
    let unequipped = world
        .get::<EquipmentSlots>(actor)
        .is_some_and(|slots| slots.equipped_indices().next().is_none());
    empty && unequipped && world.get::<EquippedWeapon>(actor).is_none()
}

fn next_actor(world: &World) -> Option<EntityId> {
    // Release the appearance query before inspecting gameplay components.
    let candidates: Vec<_> = world
        .query::<NpcLootAppearance>()?
        .iter()
        .filter(|(_, a)| !a.finished && !a.failed)
        .map(|(actor, _)| actor)
        .collect();
    candidates
        .into_iter()
        .find(|&actor| fully_looted(world, actor) && world.get::<CellRoot>(actor).is_some())
}

pub(crate) fn status(world: &World, actor: EntityId) -> String {
    let eligible = fully_looted(world, actor);
    let cell = world.get::<CellRoot>(actor).map(|cell| cell.0);
    let Some(a) = world.get::<NpcLootAppearance>(actor) else {
        return format!(
            "npc.appearance: actor={actor} recipe=none eligible={eligible} cell={cell:?}"
        );
    };
    format!("npc.appearance: actor={actor} eligible={eligible} cell={cell:?} bones={} parts={}/{} original_roots={} staged_roots={} finished={} failed={} next={:?}",
        a.skeleton.len(), a.next_part, a.parts.len(), a.original_roots.len(),
        a.staged_roots.len(), a.finished, a.failed,
        a.parts.get(a.next_part).map(|part| part.path.as_str()))
}

/// Mesh entities at or under `root`, cycle-safe. `NpcAppearanceHidden` is
/// consumed by the render passes per mesh entity, so hiding a root means
/// marking every mesh in its subtree.
fn mesh_entities_under(world: &World, root: EntityId) -> Vec<EntityId> {
    let mut pending = vec![root];
    let mut seen = HashSet::new();
    let mut meshes = Vec::new();
    while let Some(entity) = pending.pop() {
        if !seen.insert(entity) {
            continue;
        }
        if let Some(children) = world.get::<Children>(entity) {
            pending.extend(children.0.iter().copied());
        }
        if world.get::<MeshHandle>(entity).is_some() {
            meshes.push(entity);
        }
    }
    meshes
}

fn set_hidden(world: &mut World, root: EntityId, hidden: bool) {
    for entity in mesh_entities_under(world, root) {
        if hidden {
            world.insert(entity, NpcAppearanceHidden);
        } else {
            world.remove::<NpcAppearanceHidden>(entity);
        }
    }
}

/// P3 live re-equip reconcile: hide/reveal a living actor's gear meshes when
/// an [`EquipmentEventBatch`] names the item they were spawned from.
///
/// Spawn-time armor roots already carry `NpcEquipmentPart` ownership
/// (actor + inventory row + FormID), so an unequip of that row maps straight
/// onto the meshes to hide — and re-equipping the same item reveals exactly
/// what the earlier unequip hid. This is the mesh half of "wire equip/
/// unequip through the mesh attachment pipeline" for every actor whose
/// meshes exist; two halves remain deliberately out of scope:
///
/// - **Newly acquired gear** (an item the actor did not spawn wearing) has
///   no root to reveal — spawning it is the corpse-restoration machinery's
///   import path, applied to mid-life equips later.
/// - **Covered skin re-exposure** (removing a chest piece should unmask the
///   torso skin) needs biped-coverage composition; the full-strip corpse
///   path owns that today.
///
/// Dead actors are skipped: death reconciliation owns their appearance
/// lifecycle (it hides originals permanently and stages a restored body), so
/// a scripted equip event on a corpse must not resurrect gear over it.
pub(crate) fn equipment_appearance_system(world: &World, _dt: f32) {
    let Some(events) = world.query::<byroredux_scripting::EquipmentEventBatch>() else {
        return;
    };
    let changes: Vec<(EntityId, Vec<byroredux_scripting::EquipmentChange>)> = events
        .iter()
        .map(|(wearer, batch)| (wearer, batch.0.clone()))
        .collect();
    drop(events);
    if changes.is_empty() {
        return;
    }
    let Some(parts) = world.query::<NpcEquipmentPart>() else {
        return;
    };
    let gear_roots: Vec<(EntityId, EntityId, u32)> = parts
        .iter()
        .filter(|(_, part)| !part.intrinsic_skin)
        .map(|(root, part)| (part.actor, root, part.form_id))
        .collect();
    drop(parts);
    if gear_roots.is_empty() {
        return;
    }
    // Expand to (root, hide) actions and pre-walk the meshes so no storage
    // guard spans another query.
    let mut actions: Vec<(EntityId, bool)> = Vec::new();
    for (wearer, batch) in &changes {
        if world.get::<Dead>(*wearer).is_some() {
            continue;
        }
        for change in batch {
            for &(owner, root, form_id) in &gear_roots {
                if owner == *wearer && form_id == change.item_form_id {
                    actions.push((root, !change.equipped));
                }
            }
        }
    }
    if actions.is_empty() {
        return;
    }
    let walks: Vec<(Vec<EntityId>, bool)> = actions
        .iter()
        .map(|&(root, hide)| (mesh_entities_under(world, root), hide))
        .collect();
    let Some(mut hidden) = world.query_mut::<NpcAppearanceHidden>() else {
        return;
    };
    for (entities, hide) in walks {
        for entity in entities {
            if hide {
                hidden.insert(entity, NpcAppearanceHidden);
            } else {
                hidden.remove(entity);
            }
        }
    }
}

fn finish(world: &mut World, actor: EntityId) {
    if !fully_looted(world, actor) {
        return;
    }
    let Some(appearance) = world.get_mut::<NpcLootAppearance>(actor) else {
        return;
    };
    if appearance.finished || appearance.failed || appearance.next_part != appearance.parts.len() {
        return;
    }
    let old = appearance.original_roots.clone();
    let new = appearance.staged_roots.clone();
    appearance.finished = true;
    for root in old {
        set_hidden(world, root, true);
    }
    for root in new {
        set_hidden(world, root, false);
    }
    log::info!("npc.loot-appearance: restored body and hid worn gear actor={actor}");
}

/// Main-thread archive providers are opened lazily and reused across actors.
#[derive(Default)]
pub(crate) struct LootAppearanceLoader {
    providers: Option<(Vec<String>, TextureProvider, MaterialProvider)>,
}

impl LootAppearanceLoader {
    /// At most one NIF per frame, after the scheduler and save/cell drains.
    pub(crate) fn step(&mut self, world: &mut World, ctx: &mut VulkanContext) {
        let Some(actor) = next_actor(world) else {
            return;
        };
        let (part, skeleton) = {
            let appearance = world.get::<NpcLootAppearance>(actor).unwrap();
            (
                appearance.parts.get(appearance.next_part).cloned(),
                appearance.skeleton.clone(),
            )
        };
        let Some(part) = part else {
            // NIF imports queue DDS data; the frame renderer does not drain
            // it. This loader owns its batch just like the cell/loose loaders.
            if ctx.texture_registry.pending_dds_upload_count() != 0 {
                let Some(allocator) = ctx.allocator.as_ref() else {
                    world.get_mut::<NpcLootAppearance>(actor).unwrap().failed = true;
                    log::warn!(
                        "npc.loot-appearance: no texture allocator; retaining outfit actor={actor}"
                    );
                    return;
                };
                if let Err(error) = ctx.texture_registry.flush_pending_uploads(
                    &ctx.device,
                    allocator,
                    &ctx.graphics_queue,
                    ctx.transfer_pool,
                    &ctx.transfer_fence,
                ) {
                    world.get_mut::<NpcLootAppearance>(actor).unwrap().failed = true;
                    log::warn!("npc.loot-appearance: texture upload failed; retaining outfit actor={actor}: {error:#}");
                    return;
                }
            }
            if !ctx.mesh_registry.is_geometry_dirty()
                && ctx.texture_registry.pending_dds_upload_count() == 0
            {
                finish(world, actor);
            }
            return;
        };
        // A cell-owned actor is required: every newly created entity must be
        // registered for the ordinary unload path, even after partial failure.
        let Some(cell) = world.get::<CellRoot>(actor).map(|cell| cell.0) else {
            return;
        };
        if skeleton.is_empty() {
            world.get_mut::<NpcLootAppearance>(actor).unwrap().failed = true;
            log::warn!("npc.loot-appearance: missing skeleton map; retaining outfit actor={actor}");
            return;
        }
        let args = crate::cli_args::effective_args();
        if self
            .providers
            .as_ref()
            .is_none_or(|(key, _, _)| *key != args)
        {
            self.providers = Some((
                args.clone(),
                crate::asset_provider::build_texture_provider(&args),
                crate::asset_provider::build_material_provider(&args),
            ));
        }
        let (_, textures, materials) = self.providers.as_mut().unwrap();
        let Some(bytes) = textures.extract_mesh(&part.path) else {
            world.get_mut::<NpcLootAppearance>(actor).unwrap().failed = true;
            log::warn!(
                "npc.loot-appearance: missing {}; retaining outfit actor={actor}",
                part.path
            );
            return;
        };
        let first = world.next_entity_id();
        let (meshes, root, _) = load_nif_bytes_with_skeleton(
            world,
            ctx,
            &bytes,
            &part.path,
            textures,
            Some(materials),
            Some(&skeleton),
            part.tint.as_deref(),
            None,
        );
        let last = world.next_entity_id();
        crate::cell_loader::stamp_cell_root_range(world, cell, first, last);
        // A failed import may have created partial entities. Hide the entire
        // range, not just a returned root, until cell teardown can release it.
        for entity in first..last {
            if world.get::<MeshHandle>(entity).is_some() {
                world.insert(entity, NpcAppearanceHidden);
            }
        }
        if let Some(root) = root {
            super::resumable::parent_part(world, actor, root);
            let appearance = world.get_mut::<NpcLootAppearance>(actor).unwrap();
            appearance.staged_roots.push(root);
            if meshes > 0 {
                appearance.next_part += 1;
                return;
            }
        }
        world.get_mut::<NpcLootAppearance>(actor).unwrap().failed = true;
        log::warn!(
            "npc.loot-appearance: body import failed {}; retaining outfit actor={actor}",
            part.path
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (World, EntityId, EntityId, EntityId, EntityId) {
        let mut world = World::new();
        world.register::<NpcAppearanceHidden>();
        let actor = world.spawn();
        let skeleton = world.spawn();
        let old = world.spawn();
        let new = world.spawn();
        for entity in [skeleton, old, new] {
            super::super::resumable::parent_part(&mut world, actor, entity);
            world.insert(entity, MeshHandle(entity as u32));
        }
        world.insert(actor, Dead);
        world.insert(actor, CellRoot(actor));
        world.insert(actor, Inventory::new());
        world.insert(actor, EquipmentSlots::new());
        world.insert(
            actor,
            NpcLootAppearance {
                parts: vec![RestorePart::body("body.nif")],
                original_roots: vec![old],
                staged_roots: vec![new],
                next_part: 1,
                ..Default::default()
            },
        );
        set_hidden(&mut world, new, true);
        (world, actor, skeleton, old, new)
    }

    #[test]
    fn completed_replacement_switches_meshes_once_without_touching_skeleton() {
        let (mut world, actor, skeleton, old, new) = fixture();
        assert_eq!(next_actor(&world), Some(actor));
        finish(&mut world, actor);
        assert!(world.get::<NpcAppearanceHidden>(old).is_some());
        assert!(world.get::<NpcAppearanceHidden>(new).is_none());
        assert!(world.get::<NpcAppearanceHidden>(skeleton).is_none());
        assert_eq!(world.get::<Parent>(new).unwrap().0, actor);
        assert!(world.get::<Dead>(actor).is_some());
        assert!(next_actor(&world).is_none());
        finish(&mut world, actor);
        assert!(world.get::<NpcAppearanceHidden>(old).is_some());
    }

    #[test]
    fn incomplete_or_failed_body_never_hides_original_outfit() {
        let (mut world, actor, _, old, new) = fixture();
        world.get_mut::<NpcLootAppearance>(actor).unwrap().next_part = 0;
        finish(&mut world, actor);
        assert!(world.get::<NpcAppearanceHidden>(old).is_none());
        assert!(world.get::<NpcAppearanceHidden>(new).is_some());
        let appearance = world.get_mut::<NpcLootAppearance>(actor).unwrap();
        appearance.next_part = 1;
        appearance.failed = true;
        finish(&mut world, actor);
        assert!(world.get::<NpcAppearanceHidden>(old).is_none());
        assert!(next_actor(&world).is_none());
    }

    #[test]
    fn living_actors_remaining_items_and_stale_equipment_are_not_stripped() {
        let (mut world, actor, _, old, _) = fixture();
        world.remove::<Dead>(actor);
        assert!(next_actor(&world).is_none());
        world.insert(actor, Dead);
        world
            .get_mut::<Inventory>(actor)
            .unwrap()
            .push(ItemStack::new(7, 1));
        assert!(next_actor(&world).is_none());
        world.get_mut::<Inventory>(actor).unwrap().items.clear();
        world
            .get_mut::<EquipmentSlots>(actor)
            .unwrap()
            .equip(4, InventoryIndex(0));
        finish(&mut world, actor);
        assert!(next_actor(&world).is_none());
        assert!(world.get::<NpcAppearanceHidden>(old).is_none());
    }

    #[test]
    fn visibility_walk_reaches_nested_meshes_but_not_unrelated_body_parts() {
        let (mut world, actor, skeleton, old, _) = fixture();
        let child = world.spawn();
        world.insert(child, MeshHandle(99));
        add_child(&mut world, old, child);
        // A malformed child cycle must not hang the reconciliation step.
        add_child(&mut world, child, old);
        finish(&mut world, actor);
        assert!(world.get::<NpcAppearanceHidden>(child).is_some());
        assert!(world.get::<NpcAppearanceHidden>(skeleton).is_none());
        assert!(world.get::<NpcAppearanceHidden>(actor).is_none());
    }

    // ── P3 live re-equip reconcile ─────────────────────────────────────

    use byroredux_scripting::{EquipmentChange, EquipmentEventBatch};

    /// A living actor with two gear roots (FormIDs 0xAAA, 0xBBB) and one
    /// intrinsic-skin root, all carrying `NpcEquipmentPart` ownership the
    /// spawn paths stamp.
    fn equip_fixture() -> (World, EntityId, EntityId, EntityId, EntityId) {
        let mut world = World::new();
        world.register::<NpcAppearanceHidden>();
        world.register::<NpcEquipmentPart>();
        world.register::<EquipmentEventBatch>();
        let actor = world.spawn();
        let gear_a = world.spawn();
        let gear_b = world.spawn();
        let skin = world.spawn();
        for (root, form_id, intrinsic) in [
            (gear_a, 0xAAAu32, false),
            (gear_b, 0xBBB, false),
            (skin, 0xAAA, true),
        ] {
            world.insert(root, MeshHandle(root as u32));
            world.insert(
                root,
                NpcEquipmentPart {
                    actor,
                    inventory_index: None,
                    form_id,
                    intrinsic_skin: intrinsic,
                    hidden_biped_mask: 0,
                },
            );
        }
        (world, actor, gear_a, gear_b, skin)
    }

    #[test]
    fn unequip_hides_only_the_named_gear_roots() {
        let (mut world, actor, gear_a, gear_b, skin) = equip_fixture();
        world.insert(
            actor,
            EquipmentEventBatch(vec![EquipmentChange {
                item_form_id: 0xAAA,
                equipped: false,
            }]),
        );
        equipment_appearance_system(&mut world, 0.0);
        assert!(
            world.get::<NpcAppearanceHidden>(gear_a).is_some(),
            "the unequipped item's meshes must hide"
        );
        assert!(world.get::<NpcAppearanceHidden>(gear_b).is_none());
        assert!(
            world.get::<NpcAppearanceHidden>(skin).is_none(),
            "race skin is a body layer, not gear"
        );
    }

    #[test]
    fn re_equipping_reveals_what_the_unequip_hid() {
        let (mut world, actor, gear_a, _, _) = equip_fixture();
        world.insert(
            actor,
            EquipmentEventBatch(vec![EquipmentChange {
                item_form_id: 0xAAA,
                equipped: false,
            }]),
        );
        equipment_appearance_system(&mut world, 0.0);
        world.insert(
            actor,
            EquipmentEventBatch(vec![EquipmentChange {
                item_form_id: 0xAAA,
                equipped: true,
            }]),
        );
        equipment_appearance_system(&mut world, 0.0);
        assert!(
            world.get::<NpcAppearanceHidden>(gear_a).is_none(),
            "re-equipping the same item reveals its spawn-time meshes"
        );
    }

    #[test]
    fn dead_wearers_and_unknown_items_are_untouched() {
        let (mut world, actor, gear_a, _, _) = equip_fixture();
        world.insert(actor, Dead);
        world.insert(
            actor,
            EquipmentEventBatch(vec![EquipmentChange {
                item_form_id: 0xAAA,
                equipped: false,
            }]),
        );
        equipment_appearance_system(&mut world, 0.0);
        assert!(
            world.get::<NpcAppearanceHidden>(gear_a).is_none(),
            "death reconciliation owns dead actors' appearance"
        );

        let (mut world, actor, gear_a, _, _) = equip_fixture();
        world.insert(
            actor,
            EquipmentEventBatch(vec![EquipmentChange {
                item_form_id: 0xCCC,
                equipped: false,
            }]),
        );
        equipment_appearance_system(&mut world, 0.0);
        assert!(world.get::<NpcAppearanceHidden>(gear_a).is_none());
    }
}
