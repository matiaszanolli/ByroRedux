//! P3 structural step — the player's visible body.
//!
//! The playable slice's P3 tail pinned the gap: the player entity is a bare
//! Character-mode capsule, so `equipment_appearance_system` observed player
//! equip events but found no `NpcEquipmentPart` meshes to act on
//! (`docs/engine/playable-vertical-slice.md`, "Live re-equip reconcile",
//! 2026-09-20). This module closes that render-side half: it assembles the
//! player's body through the same budgeted `NpcSpawnJob` machinery every
//! placed NPC takes — race skeleton, race-skin/outfit armor meshes resolved
//! from the same `NPC_ 0x7` record `inventory::attach_to_player` already
//! consumes — then parents the assembled root under the player capsule.
//!
//! Deliberate differences from an NPC spawn (via `NpcSpawnJob`'s
//! `player_body` flag, threaded through `npc_spawn/resumable.rs`): no
//! identity stamps (the player entity already carries FormID sentinel,
//! ActorValues, Inventory/EquipmentSlots, scene-alias candidate), no
//! inventory/equipment writes, no AI package, no loot-appearance state, no
//! walk/idle clips, and no bone colliders or ragdoll template. The player's
//! physics presence stays the Character-mode capsule — the single body every
//! interaction/occlusion/combat ray excludes — so the assembled body is
//! strictly visual.
//!
//! Camera view: the character camera is pinned at the body's eye height, so
//! the body is hidden while [`PlayerCameraView::FirstPerson`] (the default)
//! and revealed in third person (V key / `player.view`). Body yaw follows
//! the same look accumulator the camera uses, so third person shows the
//! body's back. Two halves stay open and are tracked in the slice doc: no
//! third-person walk/idle animation yet (the body moves rigid with the
//! capsule), and no new-gear import for mid-life equips (spawn-time gear
//! only, same as the NPC re-equip reconcile's scope).

use std::collections::HashSet;

use byroredux_core::ecs::components::collision::{CollisionShape, RigidBodyData};
use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::{
    Children, Component, GlobalTransform, Parent, SparseSetStorage, Transform, World,
};
use byroredux_core::math::{Quat, Vec3};
use byroredux_renderer::VulkanContext;

use crate::cell_loader::{FrameTimeBudget, LoadedCellIndex, LoadedPluginSet};
use crate::components::InputState;
use crate::helpers::add_child;
use crate::npc_spawn::loot_appearance::mesh_entities_under;
use crate::npc_spawn::{NpcEquipmentPart, NpcSpawnJob, NpcSpawnProgress};
use crate::systems::PlayerMode;

/// Which side of the player the character camera views from. First person is
/// the default: the camera sits at the body's eye height and the body meshes
/// carry [`HiddenFirstPerson`] until this is switched.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum PlayerCameraView {
    #[default]
    FirstPerson,
    ThirdPerson,
}

impl Resource for PlayerCameraView {}

/// Marker on the assembled body's placement root. Presence validates the
/// [`PlayerBodyRootEntity`] resource against load/teardown edge cases: the
/// root is parented to the player (never `CellRoot`-owned), so it survives
/// cell transitions, and this check keeps a stale entity id from being
/// written to.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PlayerBodyRoot;

impl Component for PlayerBodyRoot {
    type Storage = SparseSetStorage<Self>;
}

/// The assembled body root entity, `None` until [`attach_player_body`]
/// completes.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct PlayerBodyRootEntity(pub(crate) Option<EntityId>);

impl Resource for PlayerBodyRootEntity {}

/// Marker on each mesh entity under the player body root while the camera is
/// first-person. Consumed by the render collection passes beside
/// `NpcAppearanceHidden`/`PickedUp` (skinned palettes + static draw loop).
/// Kept as a component rather than a per-frame resource consult so the two
/// render sites pay exactly the per-mesh query cost they already do, and so
/// equip-event hides (`NpcAppearanceHidden`) never fight the view toggle —
/// the two markers compose.
#[derive(Debug, Clone, Copy)]
pub(crate) struct HiddenFirstPerson;

impl Component for HiddenFirstPerson {
    type Storage = SparseSetStorage<Self>;
}

/// Imported humanoid skeletons walk with local +Z as forward (the NPC
/// locomotion turn is `atan2(Δx, Δz)` → `from_rotation_y`, which points
/// local +Z at the movement target), while the character camera looks along
/// its own -Z. Adding a half turn lines the body's +Z up with the camera
/// forward so third person shows the body's back. If a future game's rig
/// faces the other way, this is the one constant to flip.
const BODY_MODEL_FORWARD_HALF_TURN: f32 = std::f32::consts::PI;

/// Assemble the player's visual body off the resolved `NPC_ 0x7` record and
/// attach it under the player capsule entity. Synchronous: the attach runs
/// once at Character-mode spawn with an unlimited frame budget — the same
/// shape interior cell loads already use at boot. Missing prerequisites
/// (no loaded plugin index, no `NPC_ 0x7`, unsupported game) leave the
/// player a bare capsule and log why; nothing else in boot depends on the
/// body existing.
///
/// Idempotent: a completed attach stamps [`PlayerBodyRootEntity`], and a
/// later call is a no-op.
pub(crate) fn attach_player_body(
    world: &mut World,
    ctx: &mut VulkanContext,
    player: EntityId,
    body_pos: Vec3,
    capsule_half_height: f32,
    capsule_radius: f32,
) {
    if world
        .try_resource::<PlayerBodyRootEntity>()
        .is_some_and(|attached| attached.0.is_some())
    {
        return;
    }
    let Some(loaded) = world.try_resource::<LoadedCellIndex>() else {
        log::info!("Player body: no loaded plugin index — staying a bare capsule");
        return;
    };
    let index = loaded.0.clone();
    drop(loaded);
    let game = index.game;
    let player_npc_form_id = crate::inventory::player_npc_form_id(game);
    let Some(npc) = index.npcs.get(&player_npc_form_id).cloned() else {
        log::warn!(
            "Player body: NPC_ {player_npc_form_id:08X} not in the loaded plugins \
             — staying a bare capsule"
        );
        return;
    };

    // The capsule centre is the player Transform's convention; the NPC
    // assembly's placement root convention is feet-at-origin. Offset the
    // child by the capsule's half extent so the body stands on the floor
    // the capsule rests on.
    let feet_offset = capsule_half_height + capsule_radius;
    let feet_pos = Vec3::new(body_pos.x, body_pos.y - feet_offset, body_pos.z);

    // Same race resolution the reference loader feeds `NpcSpawnJob::runtime`
    // (Use-Traits terminal, so a template-authored race wins over the
    // shell's own RNAM).
    let resolved_race_form_id = byroredux_plugin::equip::resolve_inherited_traits(
        &npc,
        byroredux_plugin::esm::records::effective_actor_level(&npc),
        &index,
    )
    .race_form_id;
    let race = index.races.get(&resolved_race_form_id);

    let mut job = if game.has_runtime_facegen_recipe() {
        NpcSpawnJob::runtime(&npc, race, game, feet_pos, Quat::IDENTITY, 1.0)
    } else if game.uses_prebaked_facegen() {
        // The player's base record is owned by the first `--master` (the
        // base-game ESM every master chain starts with). The parsed
        // `LoadOrder` is not retained at spawn time, so the plugin name
        // comes off the CLI plugin set instead — same convention the
        // FaceGen path expects (lowercase basename with extension).
        let plugin = plugin_name_for_player(world);
        NpcSpawnJob::prebaked(&npc, game, &plugin, feet_pos, Quat::IDENTITY, 1.0)
    } else {
        log::info!("Player body: game takes neither spawn recipe — staying a bare capsule");
        return;
    }
    .into_player_body();

    // The interior boot path drops its providers after the cell load; the
    // corpse-appearance loader opens its own the same way. One archive
    // re-open at boot, never on the per-frame path.
    let args = crate::cli_args::effective_args();
    let tex_provider = crate::asset_provider::build_texture_provider(&args);
    let mut mat_provider = crate::asset_provider::build_material_provider(&args);

    let t0 = std::time::Instant::now();
    let mut budget = FrameTimeBudget::unlimited();
    let progress = job.advance(
        world,
        ctx,
        &tex_provider,
        Some(&mut mat_provider),
        // No idle pool: the player body spawns unanimated (see the finalize
        // skip in resumable.rs).
        &[],
        &index,
        &mut budget,
    );
    let root = match progress {
        NpcSpawnProgress::Complete(result) => result.root,
        NpcSpawnProgress::Pending => {
            unreachable!("unlimited budget must drive the job to completion")
        }
    };
    let Some(root) = root else {
        log::warn!("Player body: assembly produced no placement root — staying a bare capsule");
        return;
    };

    attach_assembled_root(world, player, root, feet_offset);
    let meshes = mesh_entities_under(world, root);
    let mesh_count = meshes.len();
    let part_count = world
        .query::<NpcEquipmentPart>()
        .map(|parts| parts.iter().filter(|(_, part)| part.actor == player).count())
        .unwrap_or(0);
    log::info!(
        "Player body: assembled from NPC_ {form:08X} ({game:?}) in {:.1} ms — \
         root={root} meshes={mesh_count} equipment_parts={part_count}",
        t0.elapsed().as_secs_f64() * 1000.0,
        form = npc.form_id,
    );
}

/// The half of the attach that needs the assembled root: parent it under the
/// player capsule with the feet offset, retarget `NpcEquipmentPart` ownership
/// to the player entity (equip events are observed on the wearer), strip any
/// bhk-derived collision components the imports created, and stamp the
/// body markers. Split from [`attach_player_body`] so tests can drive it on
/// a synthetic subtree without a Vulkan device or plugin data.
pub(crate) fn attach_assembled_root(
    world: &mut World,
    player: EntityId,
    root: EntityId,
    feet_offset: f32,
) {
    // The placement root was spawned at the feet world position. Parenting
    // makes its Transform local: identity rotation/scale, feet hanging the
    // capsule half-extent below the capsule centre. The player capsule's
    // rotation is locked to identity by the character controller, so local
    // = world here.
    world.insert(root, Parent(player));
    add_child(world, player, root);
    world.insert(root, Transform::new(Vec3::new(0.0, -feet_offset, 0.0), Quat::IDENTITY, 1.0));
    // Keep the composed pose continuous until propagation runs.
    if let Some(player_translation) = world.get::<Transform>(player).map(|t| t.translation) {
        world.insert(
            root,
            GlobalTransform::new(
                Vec3::new(
                    player_translation.x,
                    player_translation.y - feet_offset,
                    player_translation.z,
                ),
                Quat::IDENTITY,
                1.0,
            ),
        );
    }

    for entity in descendant_entities(world, root) {
        // Equip events are observed on the wearer (the player capsule), so
        // `equipment_appearance_system` matches `part.actor == player`.
        let retargeted = world.get::<NpcEquipmentPart>(entity).and_then(|part| {
            (part.actor != player).then(|| NpcEquipmentPart {
                actor: player,
                ..*part
            })
        });
        if let Some(part) = retargeted {
            world.insert(entity, part);
        }
        // Visual-only body: no bone colliders beside the capsule. The
        // skeleton phases skip the keyframe/ragdoll hooks for player bodies;
        // this catches collision the NIF bhk import itself created. Removal
        // before the first physics sync means no Rapier bodies are ever
        // registered for them.
        world.remove::<CollisionShape>(entity);
        world.remove::<RigidBodyData>(entity);
    }

    world.insert(root, PlayerBodyRoot);
    world.insert_resource(PlayerBodyRootEntity(Some(root)));

    // Boot attaches in first person (the camera is inside the body), so the
    // freshly assembled meshes start hidden. `set_player_view` owns every
    // later restamp. Direct inserts rather than a `query_mut` pass: the
    // engine registers the marker's storage at boot (#4991), but this
    // `&mut World` attach is also driven by worlds that did not (tests,
    // tooling), and `query_mut` reports None for a missing storage instead of
    // creating it.
    for entity in mesh_entities_under(world, root) {
        world.insert(entity, HiddenFirstPerson);
    }
}

/// Every entity strictly under `root`, cycle-safe. Unlike
/// `mesh_entities_under` this walks all entities (part roots are not mesh
/// entities themselves).
fn descendant_entities(world: &World, root: EntityId) -> Vec<EntityId> {
    let mut pending = vec![root];
    let mut seen = HashSet::new();
    let mut descendants = Vec::new();
    while let Some(entity) = pending.pop() {
        if !seen.insert(entity) {
            continue;
        }
        if let Some(children) = world.get::<Children>(entity) {
            pending.extend(children.0.iter().copied());
        }
        if entity != root {
            descendants.push(entity);
        }
    }
    descendants
}

/// The lowercase-with-extension plugin name for the player's FaceGen lookup.
/// The player's base record lives in the first `--master` (the base-game
/// ESM); with no master chain, the `--esm` value itself. Empty when neither
/// exists, which the FaceGen path treats as a graceful miss.
fn plugin_name_for_player(world: &World) -> String {
    let Some(plugins) = world.try_resource::<LoadedPluginSet>() else {
        return String::new();
    };
    let path = plugins
        .masters
        .first()
        .cloned()
        .unwrap_or_else(|| plugins.esm_path.clone());
    std::path::Path::new(&path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or(path)
}

/// Switch the player camera view and restamp [`HiddenFirstPerson`] on the
/// body's meshes. The only writer of both the resource and the marker.
/// `&World`-only: both resources are pre-inserted at boot (the attach and
/// the boot path never remove them), so the write goes through the
/// interior-mutability resource guard like every `&World` command handler.
pub(crate) fn set_player_view(world: &World, view: PlayerCameraView) {
    let Some(root) = world
        .try_resource::<PlayerBodyRootEntity>()
        .and_then(|attached| attached.0)
    else {
        log::info!("player.view: no player body attached — view switch has nothing to show");
        return;
    };
    let meshes = mesh_entities_under(world, root);
    if let Some(mut hidden_q) = world.query_mut::<HiddenFirstPerson>() {
        match view {
            PlayerCameraView::FirstPerson => {
                for entity in meshes {
                    hidden_q.insert(entity, HiddenFirstPerson);
                }
            }
            PlayerCameraView::ThirdPerson => {
                for entity in meshes {
                    hidden_q.remove(entity);
                }
            }
        }
    }
    if let Some(mut current) = world.try_resource_mut::<PlayerCameraView>() {
        *current = view;
    }
    log::info!("player.view: {view:?}");
}

/// V-key / `player.view` handler: flip the camera view and restamp the
/// body's first-person hiding. Character-mode only — FlyCam sessions have no
/// body to show, and the FlyCam camera owns the view while it is active.
pub(crate) fn toggle_third_person(world: &World) {
    let mode = world
        .try_resource::<PlayerMode>()
        .map(|mode| *mode)
        .unwrap_or_default();
    if mode != PlayerMode::Character {
        return;
    }
    let view = world
        .try_resource::<PlayerCameraView>()
        .map(|view| *view)
        .unwrap_or_default();
    let next = match view {
        PlayerCameraView::FirstPerson => PlayerCameraView::ThirdPerson,
        PlayerCameraView::ThirdPerson => PlayerCameraView::FirstPerson,
    };
    set_player_view(world, next);
}

/// Body facing: the body root's yaw follows the look accumulator the camera
/// reads, so third person shows the body oriented with the view. An Update
/// exclusive (#4995): `InputState.yaw` is final before the scheduler runs,
/// so writing here lets PostUpdate propagation compose the body root under
/// the (identity-rotation) capsule in the same frame the Late
/// `camera_follow_system` uses that yaw — no one-frame trail on fast turns.
pub(crate) fn player_body_facing_system(world: &World, _dt: f32) {
    let mode = world
        .try_resource::<PlayerMode>()
        .map(|mode| *mode)
        .unwrap_or_default();
    if mode != PlayerMode::Character {
        return;
    }
    let Some(root) = world
        .try_resource::<PlayerBodyRootEntity>()
        .and_then(|attached| attached.0)
    else {
        return;
    };
    if world.get::<PlayerBodyRoot>(root).is_none() {
        return;
    }
    let Some(input) = world.try_resource::<InputState>() else {
        return;
    };
    let yaw = input.yaw;
    drop(input);
    if let Some(mut tq) = world.query_mut::<Transform>() {
        if let Some(transform) = tq.get_mut(root) {
            transform.rotation = Quat::from_rotation_y(yaw + BODY_MODEL_FORWARD_HALF_TURN);
        }
    }
}

/// `player.body` diagnostics — the smoke gate's one-stop summary of the
/// attach. Read-only.
pub(crate) fn status_line(world: &World) -> String {
    let Some(root) = world
        .try_resource::<PlayerBodyRootEntity>()
        .and_then(|attached| attached.0)
    else {
        return "player.body: not attached (bare capsule)".to_string();
    };
    let meshes = mesh_entities_under(world, root);
    let hidden = meshes
        .iter()
        .filter(|entity| world.get::<HiddenFirstPerson>(**entity).is_some())
        .count();
    // The attach retargets every part's ownership to the player entity (the
    // wearer whose equip events `equipment_appearance_system` matches), so
    // the part list is "all parts owned by the player".
    let player = world
        .try_resource::<crate::systems::PlayerEntity>()
        .and_then(|player| player.0);
    let parts: Vec<String> = world
        .query::<NpcEquipmentPart>()
        .map(|parts| {
            parts
                .iter()
                .filter(|(_, part)| Some(part.actor) == player)
                .map(|(_, part)| {
                    format!(
                        "{:08X}{}",
                        part.form_id,
                        if part.intrinsic_skin { "+skin" } else { "" },
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    let skeleton = world
        .get::<crate::components::AnimationTarget>(root)
        .map(|target| format!("skeleton={}", target.skeleton_root))
        .unwrap_or_else(|| "skeleton=none".to_string());
    let view = world
        .try_resource::<PlayerCameraView>()
        .map(|view| format!("{:?}", *view))
        .unwrap_or_else(|| "unset".to_string());
    format!(
        "player.body: root={root} meshes={} hidden_first_person={} view={} {} parts=[{}]",
        meshes.len(),
        hidden,
        view,
        skeleton,
        parts.join(", "),
    )
}


#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_core::ecs::components::collision::MotionType;
    use byroredux_core::ecs::MeshHandle;

    /// A stand-in for what `NpcSpawnJob::advance` assembled: a root at the
    /// feet, one armor part root (ownership stamped on `part_actor`, the
    /// placement root in the real pipeline) owning a mesh, and one skeleton
    /// bone carrying bhk-derived collision. Pins the post-pass contract
    /// without a Vulkan device or plugin data.
    struct FakeBody {
        root: EntityId,
        part_root: EntityId,
        mesh: EntityId,
        bone: EntityId,
    }

    fn spawn_root(world: &mut World, at_feet_y: f32) -> EntityId {
        let root = world.spawn();
        world.insert(
            root,
            Transform::new(Vec3::new(0.0, at_feet_y, 0.0), Quat::IDENTITY, 1.0),
        );
        world.insert(root, GlobalTransform::IDENTITY);
        root
    }

    fn spawn_fake_body(world: &mut World, root: EntityId, part_actor: EntityId) -> FakeBody {
        let part_root = world.spawn();
        world.insert(part_root, Transform::new(Vec3::ZERO, Quat::IDENTITY, 1.0));
        world.insert(
            part_root,
            NpcEquipmentPart {
                actor: part_actor,
                inventory_index: None,
                form_id: 0x1234,
                intrinsic_skin: false,
                hidden_biped_mask: 0,
            },
        );
        add_child(world, root, part_root);

        let mesh = world.spawn();
        world.insert(mesh, Transform::new(Vec3::ZERO, Quat::IDENTITY, 1.0));
        world.insert(mesh, MeshHandle(7));
        add_child(world, part_root, mesh);

        let bone = world.spawn();
        world.insert(
            bone,
            CollisionShape::Capsule {
                half_height: 8.0,
                radius: 4.0,
            },
        );
        world.insert(
            bone,
            RigidBodyData {
                motion_type: MotionType::Keyframed,
                ..Default::default()
            },
        );
        add_child(world, root, bone);

        FakeBody {
            root,
            part_root,
            mesh,
            bone,
        }
    }

    #[test]
    fn attach_reparents_offsets_retargets_and_strips_collision() {
        let mut world = World::new();
        // Capsule centre at y=116, half extent 60+40=100 → feet at y=16,
        // which is exactly where the fake body's root is spawned below.
        let player = world.spawn();
        world.insert(
            player,
            Transform::new(Vec3::new(0.0, 116.0, 0.0), Quat::IDENTITY, 1.0),
        );
        let root = spawn_root(&mut world, 16.0);
        let body = spawn_fake_body(&mut world, root, /* part_actor: */ root);

        attach_assembled_root(&mut world, player, body.root, 100.0);

        assert_eq!(
            world.get::<Parent>(body.root).map(|p| p.0),
            Some(player),
            "the body root hangs off the player capsule"
        );
        assert_eq!(
            world
                .get::<Transform>(body.root)
                .map(|t| t.translation)
                .unwrap(),
            Vec3::new(0.0, -100.0, 0.0),
            "the root's Transform becomes the feet offset under the capsule centre"
        );
        assert_eq!(
            world.try_resource::<PlayerBodyRootEntity>().unwrap().0,
            Some(body.root)
        );
        assert!(world.get::<PlayerBodyRoot>(body.root).is_some());
        assert_eq!(
            world
                .get::<NpcEquipmentPart>(body.part_root)
                .map(|part| part.actor),
            Some(player),
            "equip events are observed on the wearer, so part ownership must \
             be retargeted from the body root to the player entity"
        );
        assert!(
            world.get::<CollisionShape>(body.bone).is_none()
                && world.get::<RigidBodyData>(body.bone).is_none(),
            "visual-only body: bone collision must never reach the physics sync"
        );
        assert_eq!(mesh_entities_under(&world, body.root).len(), 1);
    }

    #[test]
    fn view_toggle_restamps_first_person_hiding() {
        let mut world = World::new();
        let player = world.spawn();
        world.insert(player, Transform::new(Vec3::ZERO, Quat::IDENTITY, 1.0));
        let root = spawn_root(&mut world, 0.0);
        let body = spawn_fake_body(&mut world, root, root);
        world.insert_resource(PlayerCameraView::default());
        attach_assembled_root(&mut world, player, body.root, 100.0);

        // The attach leaves the body hidden (first person is the default).
        assert_eq!(world
            .try_resource::<PlayerCameraView>()
            .map(|view| *view)
            .unwrap_or(PlayerCameraView::FirstPerson), PlayerCameraView::FirstPerson);
        let hidden_count = || {
            mesh_entities_under(&world, body.root)
                .iter()
                .filter(|entity| world.get::<HiddenFirstPerson>(**entity).is_some())
                .count()
        };
        assert_eq!(hidden_count(), 1);

        set_player_view(&world, PlayerCameraView::ThirdPerson);
        assert_eq!(
            *world.try_resource::<PlayerCameraView>().unwrap(),
            PlayerCameraView::ThirdPerson
        );
        assert_eq!(hidden_count(), 0, "third person reveals the body");

        set_player_view(&world, PlayerCameraView::FirstPerson);
        assert_eq!(hidden_count(), 1, "first person hides it again");
    }

    /// #4991 — replay both halves of the render-skip lock cycle so the
    /// `BYRO_LOCK_ORDER_CHECK` lane can see `HiddenFirstPerson`. Propagation
    /// records `Children -> GlobalTransform`; the render skip sites hold
    /// `GlobalTransform` while reading the marker. If `set_player_view` ever
    /// takes the marker write before its `mesh_entities_under` walk, it
    /// records `HiddenFirstPerson -> Children` and closes
    /// `HiddenFirstPerson -> Children -> GlobalTransform -> HiddenFirstPerson`
    /// (the #4983 shape), which the detector panics on.
    #[test]
    fn view_restamp_does_not_close_the_render_skip_lock_cycle() {
        let mut world = World::new();
        world.register::<HiddenFirstPerson>();
        world.register::<GlobalTransform>();
        let player = world.spawn();
        world.insert(player, Transform::new(Vec3::ZERO, Quat::IDENTITY, 1.0));
        world.insert(player, GlobalTransform::IDENTITY);
        let root = spawn_root(&mut world, 0.0);
        let body = spawn_fake_body(&mut world, root, root);
        world.insert(body.mesh, GlobalTransform::IDENTITY);
        world.insert_resource(PlayerCameraView::default());
        attach_assembled_root(&mut world, player, body.root, 100.0);

        // Hierarchy half: Children -> GlobalTransform.
        byroredux_core::ecs::make_transform_propagation_system()(&world, 0.0);
        // Render half: the marker read under the GlobalTransform read, the
        // shape of `build_skinned_palettes` / `collect_static_mesh_draws`.
        {
            let gt = world.query::<GlobalTransform>();
            let hidden = world.query::<HiddenFirstPerson>();
            assert!(gt.is_some(), "propagated body has GlobalTransform storage");
            assert!(
                hidden.expect("marker storage registered").get(body.mesh).is_some(),
                "first person hides the body mesh the render passes skip"
            );
        }
        // Producer, both directions.
        set_player_view(&world, PlayerCameraView::ThirdPerson);
        assert!(world.get::<HiddenFirstPerson>(body.mesh).is_none());
        set_player_view(&world, PlayerCameraView::FirstPerson);
        assert!(world.get::<HiddenFirstPerson>(body.mesh).is_some());
    }

    #[test]
    fn status_line_reports_the_attach_or_its_absence() {
        let mut world = World::new();
        assert!(
            status_line(&world).contains("not attached"),
            "a bare-capsule session reports the fallback, not a stale root"
        );

        let player = world.spawn();
        world.insert(player, Transform::new(Vec3::ZERO, Quat::IDENTITY, 1.0));
        let root = spawn_root(&mut world, 0.0);
        let body = spawn_fake_body(&mut world, root, root);
        world.insert_resource(PlayerCameraView::default());
        world.insert_resource(crate::systems::PlayerEntity(Some(player)));
        attach_assembled_root(&mut world, player, body.root, 100.0);
        let line = status_line(&world);
        assert!(line.contains("meshes=1"), "{line}");
        assert!(line.contains("1234"), "{line}");
        assert!(line.contains("FirstPerson"), "{line}");
    }

    #[test]
    fn facing_system_orients_the_body_opposite_the_camera() {
        use crate::systems::PlayerMode;

        let yaw = 1.25;
        let mut world = World::new();
        world.insert_resource(PlayerMode::Character);
        world.insert_resource(InputState {
            yaw,
            ..InputState::default()
        });
        let root = world.spawn();
        world.insert(root, Transform::new(Vec3::ZERO, Quat::IDENTITY, 1.0));
        world.insert(root, PlayerBodyRoot);
        world.insert_resource(PlayerBodyRootEntity(Some(root)));

        player_body_facing_system(&world, 0.016);
        let rotation = world.get::<Transform>(root).unwrap().rotation;
        let expected = Quat::from_rotation_y(yaw + BODY_MODEL_FORWARD_HALF_TURN);
        assert!(
            rotation.angle_between(expected) < 1e-4,
            "the body's +Z (the locomotion forward convention) must point where \
             the camera looks: got {rotation:?}, want {expected:?}"
        );

        // No attach → no write, no panic (the bare-capsule session).
        let mut world = World::new();
        world.insert_resource(PlayerMode::Character);
        let root = world.spawn();
        world.insert(root, Transform::new(Vec3::ZERO, Quat::IDENTITY, 1.0));
        player_body_facing_system(&world, 0.016);
        assert_eq!(
            world.get::<Transform>(root).unwrap().rotation,
            Quat::IDENTITY
        );

        // FlyCam → the facing system is a no-op.
        let mut world = World::new();
        world.insert_resource(PlayerMode::FlyCam);
        world.insert_resource(InputState {
            yaw,
            ..InputState::default()
        });
        let root = world.spawn();
        world.insert(root, Transform::new(Vec3::ZERO, Quat::IDENTITY, 1.0));
        world.insert_resource(PlayerBodyRootEntity(Some(root)));
        player_body_facing_system(&world, 0.016);
        assert_eq!(
            world.get::<Transform>(root).unwrap().rotation,
            Quat::IDENTITY
        );
    }
}
