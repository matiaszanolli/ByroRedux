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
//! body's back. Locomotion animation (`attach_player_locomotion_animation`)
//! wires the same motion-based walk/idle playback every placed NPC takes,
//! driven off the capsule. Gear the player equips mid-life takes the same
//! path as an NPC's: `equipment_appearance_system` queues a
//! `PendingGearImport` for an equip with no spawn-time root, and
//! `GearImportLoader` imports and attaches the worn mesh (hidden in first
//! person via `HiddenFirstPerson`). One half stays open and is tracked in the
//! slice doc: player FaceGen (vanilla ships no facegeom for the player
//! record — the graceful miss leaves the race-default head).

use std::collections::HashSet;

use byroredux_core::animation::{AnimationClipRegistry, AnimationPlayer};
use byroredux_core::ecs::components::collision::{CollisionShape, RigidBodyData};
use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::{
    Children, Component, GlobalTransform, Parent, SparseSetStorage, Transform, World,
};
use byroredux_core::math::{Quat, Vec3};
use byroredux_plugin::equip::Gender;
use byroredux_plugin::esm::reader::GameKind;
use byroredux_renderer::VulkanContext;

use crate::asset_provider::TextureProvider;
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
        // No idle pool: the job attaches no idle for the player body —
        // `attach_player_locomotion_animation` below resolves it (see the
        // finalize skip in resumable.rs).
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
    attach_player_locomotion_animation(
        world,
        player,
        root,
        game,
        &npc,
        race.map(|race| race.race_flags),
        &tex_provider,
    );
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

/// P3 — third-person walk/idle animation for the assembled body.
///
/// The body finalizes with [`crate::components::AnimationTarget`] (its
/// skeleton binding) but deliberately no playback — the job's
/// `player_body` variant strips walk/idle clips. This attaches the same
/// motion-based playback every placed NPC takes, wired to the *capsule*:
/// `npc_walk_animation_system` watches a `WalkAnimation` carrier's own
/// `Transform` for per-tick XZ displacement, and the only entity whose
/// transform the character controller moves is the capsule — the body
/// root hangs under it at a fixed feet offset, so its local transform
/// never changes. The capsule therefore gets a second
/// [`crate::components::AnimationTarget`] (pointing at the body's
/// skeleton, so a walk take can bind its player), the optional idle
/// [`AnimationPlayer`], and the [`crate::components::WalkAnimation`].
///
/// Per-game shape matches NPCs exactly: KF games (Oblivion/FO3/FNV) get
/// the shared `mtidle.kf` idle player plus the gendered humanoid walk
/// clip; Skyrim+ has no ambient HKX idle, so it gets the walk clip only
/// and freezes into the standing shape when the capsule stops. Clip
/// resolution reuses the NPC spawn path's loaders (`load_idle_clip`, the
/// registry-warmed `humanoid_walk_kf_path`, `SkyrimWalkClip`) — both
/// install at cell load, which precedes this attach at boot, so neither
/// pays archive I/O here.
///
/// Deliberately no [`crate::components::WalkSpeed`]: that component feeds
/// the AI locomotion procedures' stride matching, and the capsule
/// controller owns player movement. Playback is view-independent: first
/// person hides the body's meshes (`HiddenFirstPerson` also skips their
/// palette builds), so a view toggle mid-stride reveals a body already in
/// stride rather than animating from bind pose on the toggle frame.
fn attach_player_locomotion_animation(
    world: &mut World,
    player: EntityId,
    root: EntityId,
    game: GameKind,
    npc: &byroredux_plugin::esm::records::actor::NpcRecord,
    race_flags: Option<u32>,
    tex_provider: &TextureProvider,
) {
    let Some(skeleton) = world
        .get::<crate::components::AnimationTarget>(root)
        .map(|target| target.skeleton_root)
    else {
        log::debug!("Player body: assembled root has no skeleton — staying unanimated");
        return;
    };
    world.insert(
        player,
        crate::components::AnimationTarget {
            skeleton_root: skeleton,
            consumed_idle_serial: 0,
        },
    );
    // The capsule is the wearer every equip event names (the attach
    // retargets NpcEquipmentPart ownership to it) and the entity the
    // animation/gear systems watch — so it also carries the job's body
    // class and bone map. The body root keeps its own stamps; both point
    // at one skeleton.
    if let Some(class) = world
        .get::<crate::npc_spawn::ActorBodyClass>(root)
        .map(|class| *class)
    {
        world.insert(player, class);
    }
    if let Some(bones) = world
        .get::<crate::npc_spawn::NpcSkeletonBones>(root)
        .map(|bones| bones.0.clone())
    {
        world.insert(player, crate::npc_spawn::NpcSkeletonBones(bones));
    }

    // Idle: the KF games' shared standing idle, desynced off the player
    // record like every NPC's (Skyrim+ resolves None here — its ambient
    // actors also spawn with no player at all, and the walk system's stop
    // path removes the player it inserted, restoring the standing shape).
    if let Some(handle) = crate::npc_spawn::load_idle_clip(world, tex_provider, game) {
        let duration = world
            .resource::<AnimationClipRegistry>()
            .get(handle)
            .map(|clip| clip.duration)
            .unwrap_or(0.0);
        let (start_time, speed) = crate::npc_spawn::idle_desync(npc.form_id, duration);
        let mut idle = AnimationPlayer::new(handle).with_root(skeleton);
        idle.local_time = start_time;
        idle.prev_time = start_time;
        idle.speed = speed;
        world.insert(player, idle);
    }

    // Walk: the same resolution ladder the NPC finalize uses — KF games
    // the per-body-class clip (gender from ACBS, the FNV-only child race
    // flag, exactly as `prepare_runtime_state` derives them), Skyrim+ the
    // decoded HKX walk.
    let gender = Gender::from_acbs_flags(npc.acbs_flags);
    let is_child = matches!(game, GameKind::Fallout3NV)
        && race_flags.is_some_and(|flags| flags & 0x04 != 0);
    let walk_handle = if game.has_kf_animations() {
        crate::npc_spawn::humanoid_walk_kf_path(game, gender, is_child)
            .and_then(|path| world.resource::<AnimationClipRegistry>().get_by_path(path))
    } else {
        world
            .try_resource::<crate::components::SkyrimWalkClip>()
            .and_then(|clip| clip.0)
    };
    let Some(walk_handle) = walk_handle else {
        log::debug!("Player body: no walk clip resolved — body moves rigid");
        return;
    };
    let last_pos = world
        .get::<Transform>(player)
        .map(|transform| transform.translation)
        .unwrap_or_default();
    world.insert(
        player,
        crate::components::WalkAnimation {
            walk_handle,
            walking: false,
            last_pos,
            captured: None,
            transition_secs: 0.0,
        },
    );
}

/// Every entity strictly under `root`, cycle-safe. Unlike
/// `mesh_entities_under` this walks all entities (part roots are not mesh
/// entities themselves).
fn descendant_entities(world: &World, root: EntityId) -> Vec<EntityId> {    let mut pending = vec![root];
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
    // The locomotion-playback half lives on the player capsule (the watched
    // entity), not on the body root — report both handles so a smoke can
    // gate the third-person walk/idle wiring, not just the attach.
    let anim = player
        .map(|player| {
            let walk = world
                .get::<crate::components::WalkAnimation>(player)
                .map(|walk| walk.walk_handle);
            let idle = world
                .get::<AnimationPlayer>(player)
                .map(|idle| idle.clip_handle);
            match (walk, idle) {
                (Some(walk), Some(idle)) => format!("anim=walk({walk})+idle({idle})"),
                (Some(walk), None) => format!("anim=walk({walk})"),
                (None, Some(idle)) => format!("anim=idle({idle})"),
                (None, None) => "anim=none".to_string(),
            }
        })
        .unwrap_or_else(|| "anim=none".to_string());
    let view = world
        .try_resource::<PlayerCameraView>()
        .map(|view| format!("{:?}", *view))
        .unwrap_or_else(|| "unset".to_string());
    format!(
        "player.body: root={root} meshes={} hidden_first_person={} view={} {} {} parts=[{}]",
        meshes.len(),
        hidden,
        view,
        skeleton,
        anim,
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

    // ── attach_player_locomotion_animation ──

    /// The Skyrim player record shape: male (ACBS gender bit clear),
    /// `NPC_ 0x7` — the record `attach_to_player` resolves for the body.
    fn player_npc() -> byroredux_plugin::esm::records::actor::NpcRecord {
        byroredux_plugin::esm::records::actor::NpcRecord {
            form_id: 0x7,
            acbs_flags: 0,
            ..Default::default()
        }
    }

    fn stub_clip(
        name: &str,
    ) -> byroredux_core::animation::AnimationClip {
        byroredux_core::animation::AnimationClip {
            name: name.into(),
            duration: 1.0,
            cycle_type: byroredux_core::animation::CycleType::Loop,
            frequency: 1.0,
            phase: 0.0,
            weight: 1.0,
            accum_root_name: None,
            channels: rustc_hash::FxHashMap::default(),
            float_channels: Vec::new(),
            color_channels: Vec::new(),
            bool_channels: Vec::new(),
            texture_flip_channels: Vec::new(),
            text_keys: Vec::new(),
        }
    }

    fn register_animation_storages(world: &mut World) {
        world.register::<crate::components::AnimationTarget>();
        world.register::<crate::components::WalkAnimation>();
        world.register::<AnimationPlayer>();
        world.register::<Transform>();
    }

    /// A body root carrying the finalize shape: `AnimationTarget` at the
    /// assembled skeleton. The capsule gets a Transform whose translation
    /// the walk detector must baseline.
    fn spawn_animated_body(world: &mut World) -> (EntityId, EntityId, EntityId) {
        let player = world.spawn();
        world.insert(
            player,
            Transform::new(Vec3::new(11.0, 116.0, -7.0), Quat::IDENTITY, 1.0),
        );
        let skeleton = world.spawn();
        let root = world.spawn();
        world.insert(
            root,
            crate::components::AnimationTarget {
                skeleton_root: skeleton,
                consumed_idle_serial: 0,
            },
        );
        (player, skeleton, root)
    }

    #[test]
    fn skyrim_capsule_gains_target_and_walk_from_the_clip_resource() {
        let mut world = World::new();
        register_animation_storages(&mut world);
        world.insert_resource(crate::components::SkyrimWalkClip(Some(9)));
        let (player, skeleton, root) = spawn_animated_body(&mut world);
        let provider = crate::asset_provider::build_texture_provider(&["byroredux".into()]);

        attach_player_locomotion_animation(
            &mut world,
            player,
            root,
            GameKind::Skyrim,
            &player_npc(),
            None,
            &provider,
        );

        assert_eq!(
            world
                .get::<crate::components::AnimationTarget>(player)
                .map(|target| target.skeleton_root),
            Some(skeleton),
            "the walk take binds its player via AnimationTarget on the watched \
             entity, so the capsule needs its own target at the body's skeleton"
        );
        {
            let walk = world
                .get::<crate::components::WalkAnimation>(player)
                .expect("the Skyrim walk clip resource resolves at cell load, before this attach");
            assert_eq!(walk.walk_handle, 9);
            assert!(!walk.walking);
            assert_eq!(
                walk.last_pos,
                Vec3::new(11.0, 116.0, -7.0),
                "the detector baselines the capsule's current translation"
            );
        }
        assert!(
            world.get::<AnimationPlayer>(player).is_none(),
            "Skyrim+ has no ambient HKX idle: the standing shape means no player \
             until the walk system inserts one"
        );
        assert!(
            world.get::<crate::components::WalkSpeed>(player).is_none(),
            "WalkSpeed feeds the AI locomotion procedures' stride; the capsule \
             controller owns player movement and must not grow one"
        );
        // The smoke gate reads the wiring off `player.body` — the anim field
        // must reflect the capsule's playback components, not just the attach.
        world.insert_resource(crate::systems::PlayerEntity(Some(player)));
        world.insert_resource(PlayerBodyRootEntity(Some(root)));
        let line = status_line(&world);
        assert!(
            line.contains("anim=walk(9)"),
            "status_line must report the capsule's walk clip for the smoke gate: {line}"
        );
    }

    #[test]
    fn kf_capsule_resolves_gendered_walk_and_shared_idle_from_the_registry() {
        let male_path = r"meshes\characters\_male\locomotion\male\mtforward.kf";
        let female_path = r"meshes\characters\_male\locomotion\female\mtforward.kf";
        let idle_path = r"meshes\characters\_male\locomotion\mtidle.kf";
        let mut world = World::new();
        register_animation_storages(&mut world);
        let mut registry = AnimationClipRegistry::new();
        let male_handle = registry.get_or_insert_by_path(male_path.to_string(), || stub_clip("m"));
        let female_handle =
            registry.get_or_insert_by_path(female_path.to_string(), || stub_clip("f"));
        let idle_handle = registry.get_or_insert_by_path(idle_path.to_string(), || stub_clip("i"));
        world.insert_resource(registry);
        let (player, skeleton, root) = spawn_animated_body(&mut world);
        // The registry fast path short-circuits before any archive access,
        // so the empty provider never matters.
        let provider = crate::asset_provider::build_texture_provider(&["byroredux".into()]);

        attach_player_locomotion_animation(
            &mut world,
            player,
            root,
            GameKind::Fallout3NV,
            &player_npc(),
            None,
            &provider,
        );

        assert_eq!(
            world
                .get::<crate::components::WalkAnimation>(player)
                .map(|walk| walk.walk_handle),
            Some(male_handle),
            "the male capsule takes the male locomotion variant, not the female one"
        );
        assert_ne!(male_handle, female_handle);
        let idle = world
            .get::<AnimationPlayer>(player)
            .expect("KF games get the shared standing idle");
        assert_eq!(idle.clip_handle, idle_handle);
        assert_eq!(idle.root_entity, Some(skeleton));
        assert!(
            idle.playing && (0.92..=1.08).contains(&idle.speed),
            "the idle plays desynced like every NPC's, not frozen at load"
        );
    }

    #[test]
    fn no_skeleton_or_no_clip_leaves_the_capsule_unanimated() {
        // Body root without AnimationTarget — the assembly failed to give a
        // skeleton — must not insert anything on the capsule.
        let mut world = World::new();
        register_animation_storages(&mut world);
        world.insert_resource(crate::components::SkyrimWalkClip(Some(9)));
        let player = world.spawn();
        world.insert(player, Transform::new(Vec3::ZERO, Quat::IDENTITY, 1.0));
        let bare_root = world.spawn();
        let provider = crate::asset_provider::build_texture_provider(&["byroredux".into()]);
        attach_player_locomotion_animation(
            &mut world,
            player,
            bare_root,
            GameKind::Skyrim,
            &player_npc(),
            None,
            &provider,
        );
        assert!(world.get::<crate::components::AnimationTarget>(player).is_none());
        assert!(world.get::<crate::components::WalkAnimation>(player).is_none());

        // Skeleton present but the walk resource empty (decode failed or no
        // animations archive) — the target lands, the walk does not.
        let mut world = World::new();
        register_animation_storages(&mut world);
        world.insert_resource(crate::components::SkyrimWalkClip(None));
        let (player, _skeleton, root) = spawn_animated_body(&mut world);
        attach_player_locomotion_animation(
            &mut world,
            player,
            root,
            GameKind::Skyrim,
            &player_npc(),
            None,
            &provider,
        );
        assert!(world.get::<crate::components::AnimationTarget>(player).is_some());
        assert!(world.get::<crate::components::WalkAnimation>(player).is_none());
    }
}
