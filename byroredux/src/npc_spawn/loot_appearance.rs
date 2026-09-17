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

fn set_hidden(world: &mut World, root: EntityId, hidden: bool) {
    let mut pending = vec![root];
    let mut seen = HashSet::new();
    while let Some(entity) = pending.pop() {
        if !seen.insert(entity) {
            continue;
        }
        if let Some(children) = world.get::<Children>(entity) {
            pending.extend(children.0.iter().copied());
        }
        if world.get::<MeshHandle>(entity).is_some() {
            if hidden {
                world.insert(entity, NpcAppearanceHidden);
            } else {
                world.remove::<NpcAppearanceHidden>(entity);
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
}
