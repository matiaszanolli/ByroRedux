//! Deferred appearance restoration for the current corpse take-all action.
//! Skeletons, physics and gameplay state stay in place. New body parts remain
//! hidden until the whole replacement is ready; superseded meshes are retained
//! until normal cell teardown, which owns their GPU/skin-slot lifetimes.

use super::*;
use byroredux_core::ecs::components::collision::CollisionShape;
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
pub(crate) fn mesh_entities_under(world: &World, root: EntityId) -> Vec<EntityId> {
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

/// Collision entities standing in for `root`'s placement: standalone ECS
/// entities tagged [`PhysicsSourceForm`] with the placement's form id, one
/// per collider a compound bhk shape spawned (#1698). They are deliberately
/// NOT descendants of the root — their transforms are world-composed, and
/// `Parent`-ing them would double-transform under propagation — which is
/// exactly why the `PickedUp` subtree marker cannot reach them and a taken
/// item's colliders must be swept separately (#4818).
pub(crate) fn collision_entities_of(world: &World, root: EntityId) -> Vec<EntityId> {
    use byroredux_core::ecs::components::{FormIdComponent, PhysicsSourceForm};

    let Some(root_form) = world.get::<FormIdComponent>(root).map(|form| form.0) else {
        return Vec::new();
    };
    let Some(forms) = world.query::<PhysicsSourceForm>() else {
        return Vec::new();
    };
    forms
        .iter()
        .filter(|(_, form)| form.0 == root_form)
        .map(|(entity, _)| entity)
        .collect()
}

/// #4818 — remove the taken item's bodies from the [`PhysicsWorld`] so a
/// picked-up placement stops blocking line of sight and shoving neighbours
/// around. Runs from `&World`: the write is interior-mutability resource
/// access, taken strictly after the component read guards above are
/// dropped (the `release_victim_rapier_bodies` two-phase pattern).
///
/// Collision entities whose bodies were never registered (spawned this
/// frame, physics not yet ticked) carry no [`RapierHandles`] and are left
/// for the caller that owns `&mut World` — the tombstone-restore path
/// despawns them outright; the live pickup path never sees one, because
/// selection itself proves the collider was registered (the activation ray
/// hit it).
///
/// Returns the number of bodies removed.
pub(crate) fn remove_collision_bodies(world: &World, root: EntityId) -> usize {
    use byroredux_physics::{PhysicsWorld, RapierHandles};

    let entities = collision_entities_of(world, root);
    if entities.is_empty() {
        return 0;
    }
    let handles_q = world.query::<RapierHandles>();
    let bodies: Vec<_> = entities
        .iter()
        .filter_map(|entity| handles_q.as_ref().and_then(|q| q.get(*entity)).copied())
        .collect();
    drop(handles_q);
    let Some(mut physics) = world.try_resource_mut::<PhysicsWorld>() else {
        return 0;
    };
    bodies
        .iter()
        .filter(|handles| physics.remove_body(handles.body))
        .count()
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
    // A world with no part carriers at all (nothing spawned wearing gear)
    // still reaches the mid-life queue below.
    let gear_roots: Vec<(EntityId, EntityId, u32)> = world
        .query::<NpcEquipmentPart>()
        .map(|parts| {
            parts
                .iter()
                .filter(|(_, part)| !part.intrinsic_skin)
                .map(|(root, part)| (part.actor, root, part.form_id))
                .collect()
        })
        .unwrap_or_default();
    // An empty root table skips only the hide/reveal half — a wearer with
    // no spawn-time gear at all equipping their first item is exactly the
    // mid-life import case.
    if !gear_roots.is_empty() {
        // Expand to (root, hide) actions and pre-walk the meshes so no
        // storage guard spans another query.
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
        if !actions.is_empty() {
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
    }
    queue_midlife_imports(world, &changes, &gear_roots);
}

/// P3 mid-life gear import — queue the worn-mesh NIF import for an equip of
/// an item the wearer never spawned wearing (no `NpcEquipmentPart` root
/// matches the form id). Resolution mirrors the spawn path exactly: the
/// retained [`ActorBodyClass`] (gender + race) against
/// `resolve_armor_meshes`, so a Skyrim ARMO picks the same race-matching
/// ARMAs and a legacy ARMO picks the same gendered `MODL`/`MOD3`. Dead
/// wearers stay skipped (death reconciliation owns corpse appearance), and
/// a wearer already pending one import is not re-queued.
fn queue_midlife_imports(
    world: &World,
    changes: &[(EntityId, Vec<byroredux_scripting::EquipmentChange>)],
    gear_roots: &[(EntityId, EntityId, u32)],
) {
    let mut requests: Vec<(EntityId, u32)> = Vec::new();
    for (wearer, batch) in changes {
        if world.get::<Dead>(*wearer).is_some() {
            continue;
        }
        for change in batch {
            if !change.equipped {
                continue;
            }
            let has_root = gear_roots.iter().any(|&(owner, _, form_id)| {
                owner == *wearer && form_id == change.item_form_id
            });
            if has_root {
                continue;
            }
            if !requests.iter().any(|(w, _)| w == wearer) {
                requests.push((*wearer, change.item_form_id));
            }
        }
    }
    if requests.is_empty() {
        return;
    }
    let Some(index_resource) = world.try_resource::<crate::cell_loader::LoadedCellIndex>()
    else {
        return;
    };
    let index = index_resource.0.clone();
    drop(index_resource);
    let mut inserts: Vec<(EntityId, PendingGearImport)> = Vec::new();
    for (wearer, form_id) in requests {
        if world.get::<PendingGearImport>(wearer).is_some() {
            continue;
        }
        let Some(class) = world.get::<ActorBodyClass>(wearer).map(|c| *c) else {
            continue;
        };
        let Some(item) = index.items.get(&form_id) else {
            continue;
        };
        let paths: Vec<String> =
            byroredux_plugin::equip::resolve_armor_meshes(
                item,
                class.gender,
                class.race_form_id,
                &index,
                index.game,
            )
            .into_iter()
            .map(str::to_owned)
            .collect();
        if paths.is_empty() {
            log::debug!(
                "mid-life gear: no worn mesh resolved for {form_id:08X} — nothing to import"
            );
            continue;
        }
        inserts.push((wearer, PendingGearImport { form_id, paths }));
    }
    if inserts.is_empty() {
        return;
    }
    // Interior-mutability insert (the caller is a `&World` Late system), and
    // only after every read guard above has dropped.
    if let Some(mut pending) = world.query_mut::<PendingGearImport>() {
        for (wearer, import) in inserts {
            log::info!(
                "mid-life gear: queueing import of {:08X} ({} mesh{}) on {wearer}",
                import.form_id,
                import.paths.len(),
                if import.paths.len() == 1 { "" } else { "es" },
            );
            pending.insert(wearer, import);
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

/// Look up the wearer's inventory row for a form id, so a mid-life import's
/// `NpcEquipmentPart` points at the same row the equip events name.
fn inventory_index_for(
    world: &World,
    wearer: EntityId,
    form_id: u32,
) -> Option<byroredux_core::ecs::components::InventoryIndex> {
    world.get::<Inventory>(wearer).and_then(|inventory| {
        inventory
            .items
            .iter()
            .position(|stack| stack.base_form_id == form_id)
            .map(|index| {
                byroredux_core::ecs::components::InventoryIndex(index as u32)
            })
    })
}

/// The player's body root when `wearer` is the player, `None` for any other
/// wearer. Mid-life player gear parents under the body root — not the
/// capsule — so it turns with the body's facing yaw and `set_player_view`'s
/// `HiddenFirstPerson` restamp (which walks the body root's subtree) covers
/// it exactly like spawn-time gear.
fn player_gear_parent(world: &World, wearer: EntityId) -> Option<EntityId> {
    let player = world
        .try_resource::<crate::systems::PlayerEntity>()
        .and_then(|player| player.0)?;
    (player == wearer).then_some(())?;
    world
        .try_resource::<crate::player_body::PlayerBodyRootEntity>()
        .and_then(|attached| attached.0)
}

/// P3 mid-life gear import — drains [`PendingGearImport`] at one NIF per
/// frame, mirroring [`LootAppearanceLoader`]'s provider cache and DDS-flush
/// posture. The difference is the reveal: these meshes are worn *now*, so a
/// successful import attaches immediately (no staged-hidden wait) — the only
/// hiding is the player's view gate, via the same `HiddenFirstPerson` marker
/// `set_player_view` restamps. Cell-owned wearers (NPCs) stamp the import
/// into their cell's release range; the player has no `CellRoot` — their
/// gear outlives cells exactly like the body it hangs from.
#[derive(Default)]
pub(crate) struct GearImportLoader {
    providers: Option<(Vec<String>, TextureProvider, MaterialProvider)>,
}

impl GearImportLoader {
    pub(crate) fn step(&mut self, world: &mut World, ctx: &mut VulkanContext) {
        let Some((wearer, import)) = world.query::<PendingGearImport>().and_then(|query| {
            query
                .iter()
                .next()
                .map(|(wearer, import)| (wearer, import.clone()))
        }) else {
            return;
        };
        if world.get::<Dead>(wearer).is_some() {
            // Died mid-import: death reconciliation owns the appearance now.
            world.remove::<PendingGearImport>(wearer);
            return;
        }
        let Some(bones) = world
            .get::<NpcSkeletonBones>(wearer)
            .map(|bones| bones.0.clone())
        else {
            world.remove::<PendingGearImport>(wearer);
            log::warn!("mid-life gear: no retained skeleton map on {wearer}; skipping import");
            return;
        };
        let Some(path) = import.paths.first().cloned() else {
            world.remove::<PendingGearImport>(wearer);
            return;
        };
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
        let Some(bytes) = textures.extract_mesh(&path) else {
            world.remove::<PendingGearImport>(wearer);
            log::warn!("mid-life gear: missing {path}; item {wearer} wears no mesh for it");
            return;
        };
        let first = world.next_entity_id();
        let (meshes, root, _) = load_nif_bytes_with_skeleton(
            world,
            ctx,
            &bytes,
            &path,
            textures,
            Some(materials),
            Some(&bones),
            None,
            None,
        );
        let last = world.next_entity_id();
        let Some(root) = root.filter(|_| meshes > 0) else {
            // A failed import may leave partial entities — hide the whole
            // range so nothing draws unparented (same posture as the corpse
            // loader); cell teardown releases them.
            if let Some(cell) = world.get::<CellRoot>(wearer).map(|cell| cell.0) {
                crate::cell_loader::stamp_cell_root_range(world, cell, first, last);
            }
            for entity in first..last {
                if world.get::<MeshHandle>(entity).is_some() {
                    world.insert(entity, NpcAppearanceHidden);
                }
            }
            world.remove::<PendingGearImport>(wearer);
            log::warn!("mid-life gear: import failed {path}; item stays meshless");
            return;
        };

        // Visual-only for the player: strip any bhk-derived collision the
        // import created, exactly like the body attach does.
        let parent_target = player_gear_parent(world, wearer);
        if parent_target.is_some() {
            for entity in first..last {
                world.remove::<CollisionShape>(entity);
                world.remove::<RigidBodyData>(entity);
            }
        }
        super::resumable::parent_part(world, parent_target.unwrap_or(wearer), root);
        if let Some(cell) = world.get::<CellRoot>(wearer).map(|cell| cell.0) {
            crate::cell_loader::stamp_cell_root_range(world, cell, first, last);
        }
        world.insert(
            root,
            NpcEquipmentPart {
                actor: wearer,
                inventory_index: inventory_index_for(world, wearer, import.form_id),
                form_id: import.form_id,
                intrinsic_skin: false,
                hidden_biped_mask: 0,
            },
        );

        // The player's view gate: in first person the new meshes start
        // hidden under the same marker `set_player_view` owns, so the next
        // toggle reveals them with the rest of the body. NPCs (and the
        // third-person player) wear the import immediately.
        let first_person = parent_target.is_some()
            && world
                .try_resource::<crate::player_body::PlayerCameraView>()
                .map(|view| *view == crate::player_body::PlayerCameraView::FirstPerson)
                .unwrap_or(true);
        if first_person {
            for entity in first..last {
                if world.get::<MeshHandle>(entity).is_some() {
                    world.insert(entity, crate::player_body::HiddenFirstPerson);
                }
            }
        }

        let mut remaining = import.paths;
        remaining.remove(0);
        if remaining.is_empty() {
            world.remove::<PendingGearImport>(wearer);
        } else {
            if let Some(pending) = world.get_mut::<PendingGearImport>(wearer) {
                pending.paths = remaining;
            }
            return;
        }
        // NIF imports queue DDS data; flush the batch once the last path is
        // in, so the new mesh has its textures the frame it first draws.
        if ctx.texture_registry.pending_dds_upload_count() != 0 {
            if let Some(allocator) = ctx.allocator.as_ref() {
                if let Err(error) = ctx.texture_registry.flush_pending_uploads(
                    &ctx.device,
                    allocator,
                    &ctx.graphics_queue,
                    ctx.transfer_pool,
                    &ctx.transfer_fence,
                ) {
                    log::warn!("mid-life gear: texture upload failed: {error:#}");
                }
            }
        }
        log::info!("mid-life gear: imported {path} for {wearer} (form {:08X})", import.form_id);
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

    // ── P3 mid-life gear import ──

    /// An FNV-shaped ARMO: the worn mesh lives on `common.model_path`
    /// (the legacy branch of `resolve_armor_meshes`, no ARMA dispatch).
    fn legacy_armor(form_id: u32, model_path: &str) -> byroredux_plugin::esm::records::ItemRecord {
        use byroredux_plugin::esm::records::common::CommonItemFields;
        use byroredux_plugin::esm::records::{ItemKind, ItemRecord};
        ItemRecord {
            form_id,
            common: CommonItemFields {
                model_path: model_path.to_string(),
                ..Default::default()
            },
            kind: ItemKind::Armor {
                female_model_path: String::new(),
                biped_flags: 0x4,
                dt: 0.0,
                dr: 0,
                health: 0,
                slot_mask: 0x4,
                armor_rating_x100: 0,
                armor_type: None,
                armatures: Vec::new(),
            },
        }
    }

    fn install_index(world: &mut World, form_id: u32, model_path: &str) {
        let mut index = byroredux_plugin::esm::records::EsmIndex::default();
        index.game = byroredux_plugin::esm::reader::GameKind::Fallout3NV;
        index
            .items
            .insert(form_id, legacy_armor(form_id, model_path));
        world.insert_resource(crate::cell_loader::LoadedCellIndex(Arc::new(index)));
    }

    #[test]
    fn equipped_item_without_a_root_queues_a_midlife_import() {
        use super::super::{ActorBodyClass, PendingGearImport};
        use byroredux_plugin::equip::Gender;

        let mut world = World::new();
        world.register::<EquipmentEventBatch>();
        world.register::<PendingGearImport>();
        world.register::<super::super::ActorBodyClass>();
        let wearer = world.spawn();
        world.insert(
            wearer,
            ActorBodyClass {
                gender: Gender::Male,
                race_form_id: 0xD7,
            },
        );
        world.insert(
            wearer,
            EquipmentEventBatch(vec![EquipmentChange {
                item_form_id: 0xABC,
                equipped: true,
            }]),
        );
        install_index(&mut world, 0xABC, r"meshes\armor\cuirass.nif");

        equipment_appearance_system(&world, 1.0 / 60.0);

        let pending = world
            .get::<PendingGearImport>(wearer)
            .expect("an equip with no spawn-time root must queue its import");
        assert_eq!(pending.form_id, 0xABC);
        assert_eq!(pending.paths, vec![r"meshes\armor\cuirass.nif"]);
    }

    #[test]
    fn unequip_rooted_and_dead_wearers_never_queue() {
        use super::super::{ActorBodyClass, PendingGearImport};
        use byroredux_plugin::equip::Gender;

        let mut world = World::new();
        world.register::<EquipmentEventBatch>();
        world.register::<PendingGearImport>();
        world.register::<super::super::ActorBodyClass>();
        let wearer = world.spawn();
        world.insert(
            wearer,
            ActorBodyClass {
                gender: Gender::Male,
                race_form_id: 0xD7,
            },
        );
        install_index(&mut world, 0xABC, r"meshes\armor\cuirass.nif");
        // A spawned root for the same form: the reveal path owns this equip.
        let root = world.spawn();
        world.insert(
            root,
            NpcEquipmentPart {
                actor: wearer,
                inventory_index: None,
                form_id: 0xABC,
                intrinsic_skin: false,
                hidden_biped_mask: 0,
            },
        );
        world.insert(
            wearer,
            EquipmentEventBatch(vec![EquipmentChange {
                item_form_id: 0xABC,
                equipped: true,
            }]),
        );
        equipment_appearance_system(&world, 1.0 / 60.0);
        assert!(world.get::<PendingGearImport>(wearer).is_none());

        // A dead wearer queues nothing (death reconciliation owns appearance).
        let mut world = World::new();
        world.register::<EquipmentEventBatch>();
        world.register::<PendingGearImport>();
        world.register::<super::super::ActorBodyClass>();
        world.register::<Dead>();
        let corpse = world.spawn();
        world.insert(corpse, Dead);
        world.insert(
            corpse,
            ActorBodyClass {
                gender: Gender::Male,
                race_form_id: 0xD7,
            },
        );
        install_index(&mut world, 0xABC, r"meshes\armor\cuirass.nif");
        world.insert(
            corpse,
            EquipmentEventBatch(vec![EquipmentChange {
                item_form_id: 0xABC,
                equipped: true,
            }]),
        );
        equipment_appearance_system(&world, 1.0 / 60.0);
        assert!(world.get::<PendingGearImport>(corpse).is_none());
    }

    #[test]
    fn inventory_index_lookup_maps_the_form_to_its_row() {
        use super::inventory_index_for;

        let mut world = World::new();
        let wearer = world.spawn();
        world.insert(
            wearer,
            Inventory {
                items: vec![
                    ItemStack::new(0x111, 1),
                    ItemStack::new(0xABC, 1),
                ],
            },
        );
        assert_eq!(
            inventory_index_for(&world, wearer, 0xABC)
                .map(|index| index.0),
            Some(1)
        );
        assert_eq!(inventory_index_for(&world, wearer, 0x999), None);
    }
}
