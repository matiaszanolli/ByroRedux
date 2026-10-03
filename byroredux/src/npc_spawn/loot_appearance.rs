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

/// Every entity at or under `root`, cycle-safe, `root` itself included.
/// The full-subtree shape #5028 needs: releasing an imported gear root must
/// despawn its nodes as well as its mesh entities.
pub(crate) fn subtree_entities_under(world: &World, root: EntityId) -> Vec<EntityId> {
    let mut pending = vec![root];
    let mut seen = HashSet::new();
    let mut entities = Vec::new();
    while let Some(entity) = pending.pop() {
        if !seen.insert(entity) {
            continue;
        }
        if let Some(children) = world.get::<Children>(entity) {
            pending.extend(children.0.iter().copied());
        }
        entities.push(entity);
    }
    entities
}

/// Mesh entities at or under `root`, cycle-safe. `NpcAppearanceHidden` is
/// consumed by the render passes per mesh entity, so hiding a root means
/// marking every mesh in its subtree.
pub(crate) fn mesh_entities_under(world: &World, root: EntityId) -> Vec<EntityId> {
    subtree_entities_under(world, root)
        .into_iter()
        .filter(|entity| world.get::<MeshHandle>(*entity).is_some())
        .collect()
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
/// meshes exist.
///
/// **Newly acquired gear** (an item the actor did not spawn wearing) has no
/// root to reveal, so the system ends by handing it to
/// [`queue_midlife_imports`], which queues a [`PendingGearImport`] that
/// [`GearImportLoader`] drains into an imported, attached worn mesh.
///
/// One half remains deliberately out of scope: **covered skin re-exposure**
/// (removing a chest piece should unmask the torso skin) needs
/// biped-coverage composition; the full-strip corpse path owns that today.
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
    // #5028 — the item-transfer half runs even on frames with no equip
    // events: an item can leave the inventory (drop, sell, destroy) without
    // any equip/unequip transition alongside.
    if changes.is_empty() {
        queue_gear_releases(world);
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
    queue_gear_releases(world);
}

/// #5028 — queue release of worn gear whose item left the inventory
/// entirely (an `ItemTransfer` with `added == false`, emitted by the loot /
/// pickup paths; stack rows move whole today, but the wearer's live
/// `Inventory` is consulted anyway so a future partial move cannot release
/// gear that is still held). Only wearers with **no** `CellRoot` qualify:
/// NPC mid-life imports are stamped into their cell's release range and
/// leave through cell teardown, while the player's gear hangs off the body
/// root with no range — pre-fix it stayed resident (geometry, BLAS, texture
/// refcounts) until shutdown. Dead wearers stay skipped: death
/// reconciliation owns their appearance. The drain side — despawn + GPU
/// release through the cell-teardown path — lives in
/// [`GearImportLoader::step_releases`].
fn queue_gear_releases(world: &World) {
    let Some(events) = world.query::<byroredux_scripting::ItemEventBatch>() else {
        return;
    };
    let leaves: Vec<(EntityId, Vec<u32>)> = events
        .iter()
        .map(|(wearer, batch)| {
            (
                wearer,
                batch
                    .0
                    .iter()
                    .filter(|transfer| !transfer.added)
                    .map(|transfer| transfer.item_form_id)
                    .collect::<Vec<_>>(),
            )
        })
        .filter(|(_, forms)| !forms.is_empty())
        .collect();
    drop(events);
    if leaves.is_empty() {
        return;
    }
    // (actor, root, form id) for every non-intrinsic gear root — the same
    // filter the hide/reveal half uses, so intrinsic skin can never be
    // released by an inventory event.
    let part_rows: Vec<(EntityId, EntityId, u32)> = world
        .query::<NpcEquipmentPart>()
        .map(|parts| {
            parts
                .iter()
                .filter(|(_, part)| !part.intrinsic_skin)
                .map(|(root, part)| (part.actor, root, part.form_id))
                .collect()
        })
        .unwrap_or_default();
    if part_rows.is_empty() {
        return;
    }
    // Interior-mutability insert (the caller is a `&World` Late system), and
    // only after every read guard above has dropped.
    let Some(mut releases) = world.query_mut::<crate::npc_spawn::PendingGearRelease>() else {
        return;
    };
    for (wearer, left_forms) in leaves {
        if world.get::<Dead>(wearer).is_some() {
            continue;
        }
        if world.get::<CellRoot>(wearer).is_some() {
            continue;
        }
        let Some(inventory) = world.get::<Inventory>(wearer) else {
            continue;
        };
        let to_release: Vec<u32> = left_forms
            .into_iter()
            .filter(|form_id| {
                // Still-held items (a future partial move, or a second stack
                // row of the same base form) must keep their meshes.
                inventory
                    .items
                    .iter()
                    .all(|stack| stack.base_form_id != *form_id)
                    && part_rows
                        .iter()
                        .any(|&(actor, _, part_form)| actor == wearer && part_form == *form_id)
            })
            .collect();
        if to_release.is_empty() {
            continue;
        }
        log::info!(
            "mid-life gear: item(s) left the inventory — queueing release of \
             {} worn form(s) on {wearer}",
            to_release.len(),
        );
        if let Some(pending) = releases.get_mut(wearer) {
            pending.form_ids.extend(to_release);
        } else {
            releases.insert(
                wearer,
                crate::npc_spawn::PendingGearRelease {
                    form_ids: to_release,
                },
            );
        }
    }
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

/// #5034 — post-load worn-gear reconcile. The event-driven
/// [`equipment_appearance_system`] only acts on `EquipmentEventBatch`
/// transitions, and a load emits none: the player's body root survives the
/// reload carrying the pre-load session's hide markers, and a respawned NPC
/// has `reference_state::restore` overlay the parked `EquipmentSlots` over
/// roots the record outfit just spawned. Both leave third-person gear
/// disagreeing with the restored slots while combat, `GetEquipped` and the
/// save itself agree.
///
/// Diffs the wearer's live non-intrinsic gear roots against the restored
/// `EquipmentSlots` (+ weapon slot) and produces the same three outcomes the
/// event path would, computed from state instead of transitions: reveal a
/// root whose form is equipped but hidden, hide a root whose form is no
/// longer equipped but visible, and queue a [`PendingGearImport`] for an
/// equipped form with no root at all. Dead wearers stay skipped (death
/// reconciliation owns corpse appearance), and the reveal half only lifts
/// `NpcAppearanceHidden` — which composes with the separate
/// `HiddenFirstPerson` view marker — so a load in first person cannot
/// un-hide the body. Called once per restored wearer; an import finishes
/// across frames in [`GearImportLoader::step_imports`], which chains this
/// reconcile again on completion so a wearer restored with several
/// root-less equipped forms imports them one per frame, the same cadence
/// the runtime event path gives a multi-equip burst.
pub(crate) fn reconcile_worn_gear(world: &World, wearer: EntityId) {
    if world.get::<Dead>(wearer).is_some() {
        return;
    }
    // Equipped form ids: every occupied biped slot plus the weapon slot,
    // resolved through the wearer's live inventory — a zero-count row is
    // not worn. Nothing equipped means no reconcile: a stripped actor keeps
    // exactly the bare-skin look it was saved with.
    let equipped_forms: HashSet<u32> = world
        .get::<EquipmentSlots>(wearer)
        .map(|equipment| {
            let indices = equipment
                .occupants
                .iter()
                .filter_map(|slot| *slot)
                .chain(equipment.weapon);
            world
                .get::<Inventory>(wearer)
                .map(|inventory| {
                    indices
                        .filter_map(|index| inventory.get(index))
                        .filter(|stack| stack.count > 0)
                        .map(|stack| stack.base_form_id)
                        .collect()
                })
                .unwrap_or_default()
        })
        .unwrap_or_default();
    let roots: Vec<(EntityId, u32)> = world
        .query::<NpcEquipmentPart>()
        .map(|parts| {
            parts
                .iter()
                .filter(|(_, part)| part.actor == wearer && !part.intrinsic_skin)
                .map(|(root, part)| (root, part.form_id))
                .collect()
        })
        .unwrap_or_default();
    // Hide/reveal half — only where visibility disagrees with the slots.
    // The event path always stamps a whole root's subtree uniformly, so
    // "any mesh unhidden" is the root's visibility. Walks are collected
    // before the marker write for the same guard hygiene the event path
    // uses (the write goes through interior mutability, the caller being
    // a `&World`).
    let mut walks: Vec<(Vec<EntityId>, bool)> = Vec::new();
    for (root, form_id) in &roots {
        let expected_visible = equipped_forms.contains(form_id);
        let meshes = mesh_entities_under(world, *root);
        let hidden = meshes
            .iter()
            .all(|&entity| world.get::<NpcAppearanceHidden>(entity).is_some());
        if expected_visible && hidden {
            walks.push((meshes, false));
        } else if !expected_visible && !hidden {
            walks.push((meshes, true));
        }
    }
    if !walks.is_empty() {
        let Some(mut markers) = world.query_mut::<NpcAppearanceHidden>() else {
            return;
        };
        for (entities, hide) in walks {
            for entity in entities {
                if hide {
                    markers.insert(entity, NpcAppearanceHidden);
                } else {
                    markers.remove(entity);
                }
            }
        }
    }
    // Import half — equipped forms with no root. The change list is
    // synthesized in memory, never planted as an `EquipmentEventBatch`:
    // consumers treat that marker as a runtime transition, and a load is
    // not one.
    let missing: Vec<byroredux_scripting::EquipmentChange> = equipped_forms
        .iter()
        .filter(|form_id| !roots.iter().any(|&(_, form)| form == **form_id))
        .map(|&form_id| byroredux_scripting::EquipmentChange {
            item_form_id: form_id,
            equipped: true,
        })
        .collect();
    if missing.is_empty() {
        return;
    }
    let root_triples: Vec<(EntityId, EntityId, u32)> = roots
        .into_iter()
        .map(|(root, form_id)| (wearer, root, form_id))
        .collect();
    queue_midlife_imports(world, &[(wearer, missing)], &root_triples);
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
        self.step_releases(world, ctx);
        self.step_imports(world, ctx);
    }

    /// #5028 — drain [`PendingGearRelease`]: despawn each released form's
    /// gear subtree through `cell_loader::unload::release_entities`, the
    /// same GPU-handle + despawn path cell teardown uses, so an item that
    /// left the inventory frees its geometry / BLAS / texture refcounts
    /// instead of staying hidden-resident until shutdown.
    fn step_releases(&mut self, world: &mut World, ctx: &mut VulkanContext) {
        let pending: Vec<(EntityId, Vec<u32>)> = world
            .query::<crate::npc_spawn::PendingGearRelease>()
            .map(|query| {
                query
                    .iter()
                    .map(|(wearer, release)| (wearer, release.form_ids.clone()))
                    .collect()
            })
            .unwrap_or_default();
        for (wearer, form_ids) in pending {
            world.remove::<crate::npc_spawn::PendingGearRelease>(wearer);
            let released_forms = form_ids.len();
            for form_id in form_ids {
                let roots: Vec<EntityId> = world
                    .query::<NpcEquipmentPart>()
                    .map(|parts| {
                        parts
                            .iter()
                            .filter(|&(root, part)| {
                                part.actor == wearer
                                    && part.form_id == form_id
                                    && !part.intrinsic_skin
                                    // Only un-stamped roots belong to this
                                    // path; a cell-stamped root (an NPC's)
                                    // releases with its cell range.
                                    && world.get::<CellRoot>(root).is_none()
                            })
                            .map(|(root, _)| root)
                            .collect()
                    })
                    .unwrap_or_default();
                for root in roots {
                    let victims = subtree_entities_under(world, root);
                    crate::cell_loader::unload::release_entities(world, ctx, &victims);
                }
            }
            log::info!(
                "mid-life gear: released worn meshes of {} form(s) from {wearer}",
                released_forms,
            );
        }
    }

    fn step_imports(&mut self, world: &mut World, ctx: &mut VulkanContext) {
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
            // loader); cell teardown releases them. A wearer with no cell
            // (the player) has no teardown to lean on, so #5028 releases the
            // partial range immediately instead of parking hidden entities
            // nothing can reach later.
            match world.get::<CellRoot>(wearer).map(|cell| cell.0) {
                Some(cell) => {
                    crate::cell_loader::stamp_cell_root_range(world, cell, first, last);
                    for entity in first..last {
                        if world.get::<MeshHandle>(entity).is_some() {
                            world.insert(entity, NpcAppearanceHidden);
                        }
                    }
                }
                None => {
                    let victims: Vec<EntityId> = (first..last).collect();
                    crate::cell_loader::unload::release_entities(world, ctx, &victims);
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
        // #5034 — a load can restore several equipped forms that have no
        // root, but the pending slot takes one import at a time. Re-running
        // the reconcile now (root exists, slots unchanged) queues the next
        // root-less form for the following frame — the same one-per-frame
        // cadence a multi-equip burst gets from the runtime event path.
        reconcile_worn_gear(world, wearer);
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
            world.insert(entity, MeshHandle(entity));
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
            world.insert(root, MeshHandle(root));
            world.insert(
                root,
                NpcEquipmentPart {
                    actor,
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
        equipment_appearance_system(&world, 0.0);
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
        equipment_appearance_system(&world, 0.0);
        world.insert(
            actor,
            EquipmentEventBatch(vec![EquipmentChange {
                item_form_id: 0xAAA,
                equipped: true,
            }]),
        );
        equipment_appearance_system(&world, 0.0);
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
        equipment_appearance_system(&world, 0.0);
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
        equipment_appearance_system(&world, 0.0);
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
        let mut index = byroredux_plugin::esm::records::EsmIndex {
            game: byroredux_plugin::esm::reader::GameKind::Fallout3NV,
            ..Default::default()
        };
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

    /// #5034 — the load reconcile produces the event path's three outcomes
    /// from state alone: reveal an equipped-but-hidden root, hide an
    /// unequipped-but-visible root, queue an import for an equipped form
    /// with no root — with no `EquipmentEventBatch` in the world.
    #[test]
    fn load_reconcile_diffs_roots_against_restored_slots() {
        use super::super::{ActorBodyClass, PendingGearImport};
        use byroredux_plugin::equip::Gender;

        let mut world = World::new();
        world.register::<NpcAppearanceHidden>();
        world.register::<PendingGearImport>();
        world.register::<super::super::ActorBodyClass>();
        world.register::<EquipmentEventBatch>();
        let wearer = world.spawn();
        world.insert(
            wearer,
            Inventory {
                items: vec![ItemStack::new(0xAAA, 1), ItemStack::new(0xCCC, 1)],
            },
        );
        let mut slots = EquipmentSlots::new();
        slots.equip(0b1, InventoryIndex(0));
        slots.equip_weapon(InventoryIndex(1));
        world.insert(wearer, slots);
        world.insert(
            wearer,
            ActorBodyClass {
                gender: Gender::Male,
                race_form_id: 0xD7,
            },
        );
        // 0xCCC (the wielded row) has no root: the reconcile must queue its
        // worn-mesh import, which needs an index entry to resolve a mesh
        // from.
        install_index(&mut world, 0xCCC, r"meshes\armor\gauntlet.nif");

        let mesh_under = |world: &mut World, root: EntityId, form_id: u32| {
            let mesh = world.spawn();
            super::super::resumable::parent_part(world, root, mesh);
            world.insert(mesh, MeshHandle(mesh));
            world.insert(
                root,
                NpcEquipmentPart {
                    actor: wearer,
                    form_id,
                    intrinsic_skin: false,
                    hidden_biped_mask: 0,
                },
            );
            mesh
        };

        // Equipped armor whose meshes the pre-load session hid (a saved
        // unequip-then-re-equip, or a boot --load body): must be revealed.
        let equipped_root = world.spawn();
        let equipped_mesh = mesh_under(&mut world, equipped_root, 0xAAA);
        world.insert(equipped_mesh, NpcAppearanceHidden);
        // A root for a form the restored slots no longer equip: must be
        // hidden.
        let unequipped_root = world.spawn();
        let unequipped_mesh = mesh_under(&mut world, unequipped_root, 0xBBB);

        reconcile_worn_gear(&world, wearer);

        assert!(
            world.get::<NpcAppearanceHidden>(equipped_mesh).is_none(),
            "a root whose form is equipped must be revealed"
        );
        assert!(
            world.get::<NpcAppearanceHidden>(unequipped_mesh).is_some(),
            "a root whose form is no longer equipped must be hidden"
        );
        let pending = world
            .get::<PendingGearImport>(wearer)
            .expect("an equipped form with no root must queue its import");
        assert_eq!(pending.form_id, 0xCCC);
        assert_eq!(pending.paths, vec![r"meshes\armor\gauntlet.nif"]);
        // No synthetic batch leaked into the world for other consumers.
        assert!(world.query::<EquipmentEventBatch>().unwrap().iter().next().is_none());
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

}

// ── #5028 — gear release on leaves-inventory ─────────────────────────

#[cfg(test)]
mod gear_release_tests {
    use super::*;
    use byroredux_scripting::{EquipmentEventBatch, ItemEventBatch, ItemTransfer};

    /// A cellless wearer (the player's shape) with one imported gear root
    /// (0xAAA) and one intrinsic skin root, an inventory that no longer
    /// holds 0xAAA, and a leaves-transfer for it.
    fn release_fixture() -> (World, EntityId, EntityId, EntityId) {
        let mut world = World::new();
        world.register::<NpcAppearanceHidden>();
        world.register::<NpcEquipmentPart>();
        world.register::<EquipmentEventBatch>();
        world.register::<ItemEventBatch>();
        world.register::<crate::npc_spawn::PendingGearRelease>();
        let actor = world.spawn();
        let gear = world.spawn();
        let skin = world.spawn();
        for (root, form_id, intrinsic) in [(gear, 0xAAAu32, false), (skin, 0xAAA, true)] {
            world.insert(root, MeshHandle(root));
            world.insert(
                root,
                NpcEquipmentPart {
                    actor,
                    form_id,
                    intrinsic_skin: intrinsic,
                    hidden_biped_mask: 0,
                },
            );
        }
        world.insert(actor, Inventory::new());
        world.insert(
            actor,
            ItemEventBatch(vec![ItemTransfer {
                item_form_id: 0xAAA,
                count: 1,
                added: false,
                stolen: false,
            }]),
        );
        (world, actor, gear, skin)
    }

    #[test]
    fn leaving_the_inventory_queues_release_for_cellless_wearers() {
        let (world, actor, _, _) = release_fixture();
        equipment_appearance_system(&world, 0.0);
        let pending = world
            .get::<crate::npc_spawn::PendingGearRelease>(actor)
            .expect("the left item's gear root must queue for release");
        assert_eq!(pending.form_ids, vec![0xAAA]);
    }

    #[test]
    fn cell_owned_wearers_still_release_through_their_cell_range() {
        let (mut world, actor, _, _) = release_fixture();
        world.insert(actor, CellRoot(actor));
        equipment_appearance_system(&world, 0.0);
        assert!(
            world
                .get::<crate::npc_spawn::PendingGearRelease>(actor)
                .is_none(),
            "a cell-stamped wearer's gear leaves with its cell's range"
        );
    }

    #[test]
    fn still_held_or_dead_wearers_keep_their_meshes() {
        let (mut world, actor, _, _) = release_fixture();
        world
            .get_mut::<Inventory>(actor)
            .unwrap()
            .push(ItemStack::new(0xAAA, 1));
        equipment_appearance_system(&world, 0.0);
        assert!(world.get::<crate::npc_spawn::PendingGearRelease>(actor).is_none());

        let (mut world, actor, _, _) = release_fixture();
        world.insert(actor, Dead);
        equipment_appearance_system(&world, 0.0);
        assert!(
            world.get::<crate::npc_spawn::PendingGearRelease>(actor).is_none(),
            "death reconciliation owns a dead wearer's appearance"
        );
    }

    /// The release target is the FULL subtree (nodes and meshes), not just
    /// the mesh entities — despawning only meshes would strand the gear's
    /// intermediate NiNodes under the body root forever.
    #[test]
    fn release_victims_span_the_whole_gear_subtree() {
        let (mut world, _actor, gear, _) = release_fixture();
        let node = world.spawn();
        let leaf = world.spawn();
        world.insert(leaf, MeshHandle(leaf));
        add_child(&mut world, gear, node);
        add_child(&mut world, node, leaf);
        // A malformed cycle must not hang the walk.
        add_child(&mut world, leaf, gear);

        let victims = subtree_entities_under(&world, gear);
        for expected in [gear, node, leaf] {
            assert!(
                victims.contains(&expected),
                "subtree walk must reach {expected} (got {victims:?})"
            );
        }
    }
}
