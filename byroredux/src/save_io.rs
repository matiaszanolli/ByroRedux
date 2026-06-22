//! M45 Save/Load — binary-side wiring for the [`byroredux_save`] crate.
//!
//! The save crate is engine-agnostic: it knows how to snapshot a
//! [`World`] given a [`SaveRegistry`], but only the binary sees every
//! component type, so the binary populates the registry
//! ([`build_save_registry`]) and installs it as a resource.
//!
//! ## What's wired
//!
//! - `save [slot]` — runs the pre-save validation pass, snapshots the
//!   live world, and writes a CRC-protected file atomically. Read-only
//!   against the World, so it's a plain [`ConsoleCommand`].
//! - `save.info <slot>` — decodes + verifies a slot (magic / version /
//!   CRC / schema fingerprint) and prints what it contains, without
//!   touching the live world.
//!
//! ## Live load (M45.1)
//!
//! `load <slot>` reloads the saved cell through the existing loader (full
//! GPU/physics/camera setup) and overlays the saved game-state deltas
//! keyed by stable form id (see [`execute_pending_save_loads`]). The
//! player/camera pose is restored on top of that: [`capture_player_pose`]
//! refreshes a [`PlayerPose`] resource each frame so a save records where
//! the player was standing and looking, and [`apply_player_pose`] re-places
//! the persisted player body (Character mode) or camera (FlyCam) after the
//! reload — without it, `load` always dropped the player at the cell's
//! default door spawn rather than the saved spot.

use std::path::PathBuf;

use byroredux_core::console::{CommandOutput, ConsoleCommand};
use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::World;
use byroredux_core::math::Vec3;
use byroredux_save::validate::validate_world;
use byroredux_save::{disk, encode, save_world, SaveRegistry, Snapshot};

/// The **mutable game-state** component columns a live load overlays onto
/// a reloaded cell, keyed by stable form id. Deliberately excludes
/// structural/identity columns (`Name` / `Parent` / `Children` / the
/// form-id key) — the reloaded cell already owns those; only post-spawn
/// *changes* (moved objects, inventory, equip, light/anim/script state)
/// are replayed.
const MUTABLE_DELTA_COLUMNS: &[&str] = &[
    "Transform",
    "Inventory",
    "EquipmentSlots",
    "LightSource",
    "LightFlicker",
    "AnimationPlayer",
    "AnimationStack",
    "ScriptTimer",
];

/// The player's standing position + look direction at save time, so a
/// live `load` can put the player back where they were rather than at the
/// reloaded cell's default door spawn.
///
/// `position` is the **engine Y-up world position** of the body in
/// Character mode (the camera is re-pinned to body + eye-height the next
/// frame by `camera_follow_system`) or of the camera itself in FlyCam
/// mode. `yaw`/`pitch` are the [`InputState`](crate::components::InputState)
/// look angles — the source of truth in *both* modes, since both camera
/// systems rebuild the camera rotation from them every frame, so a saved
/// `Transform.rotation` alone wouldn't survive a tick.
///
/// Refreshed every frame by [`capture_player_pose`]; registered as a save
/// resource so it rides along in the snapshot; re-applied on load by
/// [`apply_player_pose`].
#[derive(Debug, Clone, Copy, Default, serde::Serialize, serde::Deserialize)]
pub struct PlayerPose {
    pub position: [f32; 3],
    pub yaw: f32,
    pub pitch: f32,
    /// `true` when captured in Character mode (body-driven), `false` for
    /// FlyCam — tells the restore which entity the `position` refers to.
    pub character_mode: bool,
}

impl Resource for PlayerPose {}

/// Where save slots live, plus the round-robin ring cursor.
///
/// Installed as a resource at startup. Default root is `<cwd>/saves`.
pub struct SaveState {
    pub dir: PathBuf,
    pub ring: disk::SaveRing,
}

impl Resource for SaveState {}

impl SaveState {
    pub fn new(dir: PathBuf, ring_size: u32) -> Self {
        Self {
            dir,
            ring: disk::SaveRing::new(ring_size),
        }
    }
}

/// Queued live-load request: a decoded, container-verified snapshot
/// awaiting the next frame's `&mut World + &mut VulkanContext` drain.
///
/// The `load` console command (which holds only `&World`) decodes + pushes
/// here; [`execute_pending_save_loads`] consumes it between frames, where
/// the App has the mutable access the cell reload needs. Mirrors the
/// `PendingDebugLoadSlot` / `PendingCellTransitionSlot` deferred shape.
#[derive(Default)]
pub struct PendingSaveLoadSlot(pub Option<Snapshot>);

impl Resource for PendingSaveLoadSlot {}

/// Build the curated game-state save registry.
///
/// Only types that carry *player-visible game state* are registered —
/// derived data (`GlobalTransform`, `WorldBound`), GPU handles
/// (`MeshHandle`, `TextureHandle`, `SkinnedMesh`), and transient event
/// markers are reconstructed on load, never serialised.
pub fn build_save_registry() -> SaveRegistry {
    use byroredux_core::animation::{AnimationPlayer, AnimationStack};
    use byroredux_core::ecs::components::{
        Children, EquipmentSlots, Inventory, LightFlicker, LightSource, Name, Parent, Transform,
    };
    use byroredux_core::ecs::resources::ItemInstancePool;
    use byroredux_scripting::ScriptTimer;

    use crate::cell_loader::CurrentCellContext;

    let mut r = SaveRegistry::new();
    r.register_component::<Transform>("Transform")
        .register_component::<Name>("Name")
        .register_component::<Parent>("Parent")
        .register_component::<Children>("Children")
        .register_component::<Inventory>("Inventory")
        .register_component::<EquipmentSlots>("EquipmentSlots")
        .register_component::<LightSource>("LightSource")
        .register_component::<LightFlicker>("LightFlicker")
        .register_component::<AnimationPlayer>("AnimationPlayer")
        .register_component::<AnimationStack>("AnimationStack")
        .register_component::<ScriptTimer>("ScriptTimer")
        .register_form_id_component("FormIdComponent")
        .register_resource::<ItemInstancePool>("ItemInstancePool")
        // M45.1 — the cell identity + plugin set the save was taken in,
        // so `load` knows which cell to reload before applying deltas.
        .register_resource::<CurrentCellContext>("CurrentCellContext")
        // M45.1 refinement — where the player was standing + looking, so
        // `load` restores the pose instead of the cell's default spawn.
        .register_resource::<PlayerPose>("PlayerPose");
    r
}

/// Pull the saved cell context out of a decoded snapshot, if present.
///
/// Returns `None` for saves taken outside an interior cell (loose-NIF /
/// exterior modes never set `CurrentCellContext`).
pub fn snapshot_cell_context(
    snap: &byroredux_save::Snapshot,
) -> Option<crate::cell_loader::CurrentCellContext> {
    snap.resources
        .get("CurrentCellContext")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
}

/// Pull the saved [`PlayerPose`] out of a decoded snapshot, if present.
///
/// Absent for pre-refinement saves (schema-fingerprint drift would reject
/// those before this is reached anyway) and for any snapshot taken with no
/// `PlayerPose` resource installed.
pub fn snapshot_player_pose(snap: &Snapshot) -> Option<PlayerPose> {
    snap.resources
        .get("PlayerPose")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
}

/// Refresh the [`PlayerPose`] resource from the live player each frame.
///
/// Called App-side in the post-scheduler phase (after the camera systems
/// have published this frame's pose), so it reads `&World` and writes the
/// resource through interior mutability — no scheduler-access declaration
/// needed. No-op until the [`PlayerPose`] resource is installed; leaves the
/// last good pose untouched if the position source can't be resolved (e.g.
/// the camera entity has no `Transform` yet).
pub fn capture_player_pose(world: &World) {
    use byroredux_core::ecs::{ActiveCamera, Transform};

    let Some(mut pose) = world.try_resource_mut::<PlayerPose>() else {
        return;
    };
    let character_mode = world
        .try_resource::<crate::systems::PlayerMode>()
        .map(|m| *m == crate::systems::PlayerMode::Character)
        .unwrap_or(false);
    let (yaw, pitch) = world
        .try_resource::<crate::components::InputState>()
        .map(|i| (i.yaw, i.pitch))
        .unwrap_or((0.0, 0.0));

    // Position source: the body in Character mode, the camera in FlyCam.
    let target = if character_mode {
        world
            .try_resource::<crate::systems::PlayerEntity>()
            .and_then(|r| r.0)
    } else {
        world.try_resource::<ActiveCamera>().map(|a| a.0)
    };
    let pos = target.and_then(|e| {
        world
            .query::<Transform>()
            .and_then(|q| q.get(e).map(|t| t.translation))
    });

    if let Some(pos) = pos {
        pose.position = pos.to_array();
        pose.yaw = yaw;
        pose.pitch = pitch;
        pose.character_mode = character_mode;
    }
}

/// Re-place the player at a saved [`PlayerPose`] after a live load.
///
/// `yaw`/`pitch` go onto [`InputState`](crate::components::InputState) in
/// both modes — that's what the camera systems read to rebuild the camera
/// rotation. The position is applied to whichever entity the active mode
/// drives: the persisted body (Character — `camera_follow_system` re-pins
/// the camera next frame) or the camera directly (FlyCam). Falls back to
/// the camera when Character mode was saved but no player body is live
/// (e.g. a `--fly` reload), so the look direction is at least honoured.
pub fn apply_player_pose(world: &mut World, pose: &PlayerPose) {
    use byroredux_core::ecs::{GlobalTransform, Transform};
    use byroredux_core::math::Quat;

    if let Some(mut input) = world.try_resource_mut::<crate::components::InputState>() {
        input.yaw = pose.yaw;
        input.pitch = pose.pitch;
    }

    let pos = Vec3::from_array(pose.position);
    let body = world
        .try_resource::<crate::systems::PlayerEntity>()
        .and_then(|r| r.0);
    let character_now = world
        .try_resource::<crate::systems::PlayerMode>()
        .map(|m| *m == crate::systems::PlayerMode::Character)
        .unwrap_or(false);

    if pose.character_mode && character_now {
        if let Some(body) = body {
            if let Some(mut tq) = world.query_mut::<Transform>() {
                if let Some(t) = tq.get_mut(body) {
                    t.translation = pos;
                }
            }
            if let Some(mut gq) = world.query_mut::<GlobalTransform>() {
                if let Some(g) = gq.get_mut(body) {
                    g.translation = pos;
                }
            }
            // Clear momentum so the body doesn't carry stale free-fall
            // velocity into the reloaded cell; gravity re-engages next tick.
            if let Some(mut cq) = world.query_mut::<byroredux_physics::CharacterController>() {
                if let Some(c) = cq.get_mut(body) {
                    c.vertical_velocity = 0.0;
                    c.is_grounded = false;
                    c.wants_jump = false;
                }
            }
            // Sync the kinematic Rapier body so the KCC's next collide-and-
            // slide starts from the restored spot (no-op without handles).
            byroredux_physics::set_kinematic_translation(world, body, pos);
            return;
        }
    }

    // FlyCam, or Character-saved with no live body: drop the camera at the
    // saved spot with a yaw/pitch-derived rotation.
    let rot = Quat::from_rotation_y(pose.yaw) * Quat::from_rotation_x(pose.pitch);
    crate::cell_loader::reposition_camera(world, pos, rot);
}

/// `save [slot]` — validate, snapshot, and atomically write the world.
pub struct SaveCommand;

impl ConsoleCommand for SaveCommand {
    fn name(&self) -> &str {
        "save"
    }
    fn description(&self) -> &str {
        "save [slot] — validate + snapshot the world to a slot (default: next ring slot)"
    }
    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let Some(registry) = world.try_resource::<SaveRegistry>() else {
            return CommandOutput::error("save registry not installed");
        };
        let Some(mut state) = world.try_resource_mut::<SaveState>() else {
            return CommandOutput::error("save directory not installed");
        };

        // Explicit slot, or advance the ring so the previous save survives.
        let slot = match args.trim() {
            "" => state.ring.advance(),
            s => match s.parse::<u32>() {
                Ok(n) => n,
                Err(_) => return CommandOutput::error(format!("invalid slot '{s}'")),
            },
        };

        // Pre-save validation — refuse to persist a broken world.
        let issues = validate_world(world);
        if !issues.is_empty() {
            let mut lines = vec![format!(
                "save ABORTED: {} referential-integrity issue(s) — refusing to write a poisoned save:",
                issues.len()
            )];
            for issue in issues.iter().take(20) {
                lines.push(format!("  [{:?}] entity {}: {}", issue.kind, issue.entity, issue.detail));
            }
            if issues.len() > 20 {
                lines.push(format!("  … and {} more", issues.len() - 20));
            }
            return CommandOutput::lines(lines);
        }

        let snapshot = match save_world(world, &registry) {
            Ok(s) => s,
            Err(e) => return CommandOutput::error(format!("snapshot failed: {e}")),
        };
        let bytes = match encode(&snapshot, registry.schema_fingerprint()) {
            Ok(b) => b,
            Err(e) => return CommandOutput::error(format!("encode failed: {e}")),
        };
        match disk::write_slot(&state.dir, slot, &bytes) {
            Ok(path) => CommandOutput::lines(vec![
                format!("saved slot {slot} → {}", path.display()),
                format!(
                    "  {} entities-worth of rows across {} component columns, {} resource(s)",
                    snapshot.row_count(),
                    snapshot.components.len(),
                    snapshot.resources.len()
                ),
                format!("  {} bytes (next_entity={})", bytes.len(), snapshot.next_entity),
            ]),
            Err(e) => CommandOutput::error(format!("write failed: {e}")),
        }
    }
}

/// `save.info <slot>` — decode + verify a slot and report its contents,
/// without mutating the live world.
pub struct SaveInfoCommand;

impl ConsoleCommand for SaveInfoCommand {
    fn name(&self) -> &str {
        "save.info"
    }
    fn description(&self) -> &str {
        "save.info <slot> — verify (magic/version/CRC/schema) + summarise a save slot"
    }
    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let Some(registry) = world.try_resource::<SaveRegistry>() else {
            return CommandOutput::error("save registry not installed");
        };
        let Some(state) = world.try_resource::<SaveState>() else {
            return CommandOutput::error("save directory not installed");
        };
        let slot = match args.trim().parse::<u32>() {
            Ok(n) => n,
            Err(_) => {
                let slots = disk::list_slots(&state.dir);
                return CommandOutput::lines(vec![
                    "usage: save.info <slot>".to_string(),
                    format!("available slots: {slots:?}"),
                ]);
            }
        };

        let bytes = match disk::read_slot(&state.dir, slot) {
            Ok(b) => b,
            Err(e) => return CommandOutput::error(format!("read slot {slot}: {e}")),
        };
        match byroredux_save::decode(&bytes, registry.schema_fingerprint()) {
            Ok(snap) => {
                let mut lines = vec![
                    format!("slot {slot}: VALID ({} bytes)", bytes.len()),
                    format!(
                        "  next_entity={}, {} strings, {} rows",
                        snap.next_entity,
                        snap.strings.len(),
                        snap.row_count()
                    ),
                ];
                match snapshot_cell_context(&snap) {
                    Some(ctx) => lines.push(format!(
                        "  cell: {} (esm {}, {} master(s))",
                        ctx.cell_editor_id,
                        ctx.esm_path,
                        ctx.masters.len()
                    )),
                    None => lines.push("  cell: <none — loose/exterior save>".to_string()),
                }
                if let Some(pose) = snapshot_player_pose(&snap) {
                    lines.push(format!(
                        "  player: ({:.1}, {:.1}, {:.1}) yaw={:.2} pitch={:.2} ({})",
                        pose.position[0],
                        pose.position[1],
                        pose.position[2],
                        pose.yaw,
                        pose.pitch,
                        if pose.character_mode { "character" } else { "flycam" },
                    ));
                }
                for (name, col) in &snap.components {
                    let rows = col.as_array().map_or(0, |a| a.len());
                    lines.push(format!("  {name}: {rows} rows"));
                }
                for name in snap.resources.keys() {
                    lines.push(format!("  resource {name}"));
                }
                CommandOutput::lines(lines)
            }
            Err(e) => CommandOutput::error(format!("slot {slot} INVALID: {e}")),
        }
    }
}

/// `load <slot>` — decode + verify a slot and queue it for the next
/// frame's live-load drain.
///
/// Read-only here (holds `&World`): it decodes, validates the container,
/// checks the snapshot carries a cell context (live load needs a cell to
/// reload), and pushes the snapshot into [`PendingSaveLoadSlot`].
/// [`execute_pending_save_loads`] does the actual cell reload + delta
/// apply between frames.
pub struct LoadCommand;

impl ConsoleCommand for LoadCommand {
    fn name(&self) -> &str {
        "load"
    }
    fn description(&self) -> &str {
        "load <slot> — reload the saved cell and apply saved game-state deltas"
    }
    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let Some(registry) = world.try_resource::<SaveRegistry>() else {
            return CommandOutput::error("save registry not installed");
        };
        let Some(state) = world.try_resource::<SaveState>() else {
            return CommandOutput::error("save directory not installed");
        };
        let slot = match args.trim().parse::<u32>() {
            Ok(n) => n,
            Err(_) => return CommandOutput::error("usage: load <slot>"),
        };

        let bytes = match disk::read_slot(&state.dir, slot) {
            Ok(b) => b,
            Err(e) => return CommandOutput::error(format!("read slot {slot}: {e}")),
        };
        let snapshot = match byroredux_save::decode(&bytes, registry.schema_fingerprint()) {
            Ok(s) => s,
            Err(e) => return CommandOutput::error(format!("slot {slot} INVALID: {e}")),
        };
        let Some(ctx) = snapshot_cell_context(&snapshot) else {
            return CommandOutput::error(
                "save has no cell context (loose/exterior save) — live load needs an interior cell",
            );
        };

        // Queue for the between-frames drain (needs &mut World + renderer).
        match world.try_resource_mut::<PendingSaveLoadSlot>() {
            Some(mut pending) => {
                pending.0 = Some(snapshot);
                CommandOutput::line(format!(
                    "queued load of slot {slot} → cell {} (applies next frame)",
                    ctx.cell_editor_id
                ))
            }
            None => CommandOutput::error("load slot not installed"),
        }
    }
}

/// Drain a queued live-load: reload the saved interior cell via the
/// existing loader (full GPU/physics/camera setup), restore saved
/// resources, then overlay the form-id-keyed mutable component deltas.
///
/// Runs once per frame after `step_debug_loads`. No-op when nothing is
/// queued. Mirrors [`crate::debug_load::execute_pending_debug_loads`]'s
/// synchronous loader-in-drain shape.
pub fn execute_pending_save_loads(
    world: &mut World,
    ctx: &mut byroredux_renderer::VulkanContext,
    streaming: &mut Option<crate::streaming::WorldStreamingState>,
) {
    let snapshot = {
        let Some(mut slot) = world.try_resource_mut::<PendingSaveLoadSlot>() else {
            return;
        };
        match slot.0.take() {
            Some(s) => s,
            None => return,
        }
    };
    let Some(cell_ctx) = snapshot_cell_context(&snapshot) else {
        log::error!("save load: snapshot lost its cell context between queue and drain");
        return;
    };

    let registry = build_save_registry();

    // Build asset providers from the boot CLI args (same BSAs the engine
    // is running with) — matches the cell-transition path.
    let args = crate::cli_args::effective_args();
    let tex_provider = crate::asset_provider::build_texture_provider(&args);
    let mut mat_provider = crate::asset_provider::build_material_provider(&args);

    // Tear down whatever's loaded, then reload the saved cell fresh.
    if streaming.is_some() {
        crate::streaming_helpers::drain_streaming_state(world, ctx, streaming);
    }
    crate::cell_loader::unload_current_interior(world, ctx);

    let result = crate::cell_loader::load_cell_with_masters(
        &cell_ctx.masters,
        &cell_ctx.esm_path,
        &cell_ctx.cell_editor_id,
        world,
        ctx,
        &tex_provider,
        Some(&mut mat_provider),
    );
    let entity_count = match result {
        Ok(r) => {
            if let Some(ref lit) = r.lighting {
                crate::cell_loader::apply_interior_cell_lighting(world, lit);
            }
            ctx.signal_temporal_discontinuity(
                crate::streaming_helpers::SVGF_TAA_STREAMING_RECOVERY_FRAMES,
            );
            world.insert_resource(crate::cell_loader::LoadedPluginSet {
                masters: cell_ctx.masters.clone(),
                esm_path: cell_ctx.esm_path.clone(),
            });
            r.entity_count
        }
        Err(e) => {
            log::error!(
                "save load: failed to reload cell '{}': {:#}",
                cell_ctx.cell_editor_id,
                e
            );
            return;
        }
    };

    // Restore saved resources (ItemInstancePool) so inventory instance
    // ids resolve, then overlay the form-id-keyed mutable deltas.
    if let Err(e) = byroredux_save::restore_resources(world, &registry, &snapshot) {
        log::error!("save load: resource restore failed: {e}");
        return;
    }
    let remap = byroredux_save::build_form_id_remap(world, &registry, &snapshot);
    match byroredux_save::apply_deltas(world, &registry, &snapshot, &remap, MUTABLE_DELTA_COLUMNS) {
        Ok(applied) => log::info!(
            "save load: cell '{}' reloaded ({} entities); applied {} saved deltas across {} \
             form-id-matched entities",
            cell_ctx.cell_editor_id,
            entity_count,
            applied,
            remap.len()
        ),
        Err(e) => log::error!("save load: delta apply failed: {e}"),
    }

    // M45.1 refinement — put the player back where they saved, on top of
    // the reloaded cell (which spawns the player at the default door).
    if let Some(pose) = snapshot_player_pose(&snapshot) {
        apply_player_pose(world, &pose);
        log::info!(
            "save load: restored player pose at ({:.1}, {:.1}, {:.1}) yaw={:.2} pitch={:.2} ({})",
            pose.position[0],
            pose.position[1],
            pose.position[2],
            pose.yaw,
            pose.pitch,
            if pose.character_mode { "character" } else { "flycam" },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_core::ecs::components::Transform;
    use byroredux_core::form_id::FormIdPool;
    use byroredux_core::math::Vec3;
    use byroredux_core::string::StringPool;
    use byroredux_save::{decode, restore_world};
    use byroredux_scripting::ScriptTimer;

    /// The binary's curated registry must round-trip its full type set —
    /// including the cross-crate `ScriptTimer` and a stable form id —
    /// through encode → decode → restore into a fresh World.
    #[test]
    fn binary_registry_round_trips_including_scripttimer() {
        let reg = build_save_registry();

        let mut src = World::new();
        src.insert_resource(StringPool::new());
        src.insert_resource(FormIdPool::new());
        let e = src.spawn();
        let other = src.spawn();
        src.insert(e, Transform::from_translation(Vec3::new(4.0, 5.0, 6.0)));
        src.insert(e, ScriptTimer { id: 42, remaining: 3.5 });
        src.insert(other, ScriptTimer { id: 7, remaining: 0.25 });

        let snap = save_world(&src, &reg).unwrap();
        let bytes = encode(&snap, reg.schema_fingerprint()).unwrap();
        let decoded = decode(&bytes, reg.schema_fingerprint()).unwrap();

        let mut dst = World::new();
        dst.insert_resource(FormIdPool::new());
        restore_world(&mut dst, &reg, &decoded).unwrap();

        assert_eq!(dst.next_entity_id(), 2);
        let q = dst.query::<ScriptTimer>().unwrap();
        let timers: std::collections::HashMap<u32, (u32, f32)> =
            q.iter().map(|(en, t)| (en, (t.id, t.remaining))).collect();
        assert_eq!(timers[&0], (42, 3.5));
        assert_eq!(timers[&1], (7, 0.25));

        let qt = dst.query::<Transform>().unwrap();
        assert_eq!(qt.iter().next().unwrap().1.translation, Vec3::new(4.0, 5.0, 6.0));
    }

    /// A clean validation pass is the precondition every save checks.
    #[test]
    fn fresh_world_validates_clean() {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, Transform::default());
        assert!(validate_world(&world).is_empty());
    }

    /// `save` then `load` (commands) round-trip through disk: the save
    /// captures the live `CurrentCellContext`, and `load` decodes it back
    /// and queues a snapshot whose cell context matches. Exercises the
    /// command plumbing end-to-end minus the GPU drain.
    #[test]
    fn save_then_load_command_queues_with_cell_context() {
        use crate::cell_loader::CurrentCellContext;

        let mut world = World::new();
        world.insert_resource(StringPool::new());
        world.insert_resource(FormIdPool::new());
        world.insert_resource(build_save_registry());
        let dir = std::env::temp_dir().join(format!("byro_m451_cmd_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        world.insert_resource(SaveState::new(dir.clone(), 4));
        world.insert_resource(PendingSaveLoadSlot::default());
        world.insert_resource(CurrentCellContext {
            cell_editor_id: "GSDocMitchellHouse".to_string(),
            esm_path: "FalloutNV.esm".to_string(),
            masters: vec![],
        });

        let e = world.spawn();
        world.insert(e, Transform::from_translation(Vec3::new(7.0, 8.0, 9.0)));

        // save → slot 0
        let out = SaveCommand.execute(&world, "0");
        assert!(
            out.lines.iter().any(|l| l.contains("saved slot 0")),
            "save output: {:?}",
            out.lines
        );

        // load → should queue a snapshot carrying the cell context
        let out = LoadCommand.execute(&world, "0");
        assert!(
            out.lines.iter().any(|l| l.contains("GSDocMitchellHouse")),
            "load output: {:?}",
            out.lines
        );
        let pending = world.resource::<PendingSaveLoadSlot>();
        let snap = pending.0.as_ref().expect("snapshot queued");
        let ctx = snapshot_cell_context(snap).expect("cell context survived round-trip");
        assert_eq!(ctx.cell_editor_id, "GSDocMitchellHouse");
        assert_eq!(ctx.esm_path, "FalloutNV.esm");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// FlyCam round-trip: capture reads the camera Transform + look
    /// angles into [`PlayerPose`]; apply puts them back after the pose is
    /// scrambled — the camera returns to the saved spot and InputState to
    /// the saved yaw/pitch.
    #[test]
    fn player_pose_round_trips_flycam() {
        use crate::components::InputState;
        use crate::systems::PlayerMode;
        use byroredux_core::ecs::{ActiveCamera, Transform};

        let mut world = World::new();
        world.insert_resource(PlayerMode::FlyCam);
        world.insert_resource(PlayerPose::default());
        world.insert_resource(InputState {
            yaw: 1.25,
            pitch: -0.4,
            ..InputState::default()
        });

        let cam = world.spawn();
        world.insert(cam, Transform::from_translation(Vec3::new(10.0, 20.0, 30.0)));
        world.insert_resource(ActiveCamera(cam));

        capture_player_pose(&world);
        let pose = *world.resource::<PlayerPose>();
        assert_eq!(pose.position, [10.0, 20.0, 30.0]);
        assert!((pose.yaw - 1.25).abs() < 1e-6);
        assert!((pose.pitch + 0.4).abs() < 1e-6);
        assert!(!pose.character_mode);

        // Scramble, then restore.
        {
            let mut tq = world.query_mut::<Transform>().unwrap();
            tq.get_mut(cam).unwrap().translation = Vec3::ZERO;
        }
        {
            let mut i = world.resource_mut::<InputState>();
            i.yaw = 0.0;
            i.pitch = 0.0;
        }
        apply_player_pose(&mut world, &pose);

        let tq = world.query::<Transform>().unwrap();
        assert_eq!(tq.get(cam).unwrap().translation, Vec3::new(10.0, 20.0, 30.0));
        let i = world.resource::<InputState>();
        assert!((i.yaw - 1.25).abs() < 1e-6);
        assert!((i.pitch + 0.4).abs() < 1e-6);
    }

    /// Character mode keys the captured position off the player *body*
    /// (not the camera), and apply moves that body — the camera follows
    /// it the next frame via `camera_follow_system`.
    #[test]
    fn player_pose_character_tracks_body() {
        use crate::components::InputState;
        use crate::systems::{PlayerEntity, PlayerMode};
        use byroredux_core::ecs::{GlobalTransform, Transform};
        use byroredux_core::math::Quat;

        let mut world = World::new();
        world.insert_resource(PlayerMode::Character);
        world.insert_resource(PlayerPose::default());
        world.insert_resource(InputState::default());
        let body = world.spawn();
        world.insert(body, Transform::from_translation(Vec3::new(-5.0, 64.0, 12.0)));
        world.insert(body, GlobalTransform::new(Vec3::new(-5.0, 64.0, 12.0), Quat::IDENTITY, 1.0));
        world.insert_resource(PlayerEntity(Some(body)));

        capture_player_pose(&world);
        let pose = *world.resource::<PlayerPose>();
        assert_eq!(pose.position, [-5.0, 64.0, 12.0]);
        assert!(pose.character_mode);

        // Apply a different saved pose; the body relocates (no Rapier
        // handles in the test → `set_kinematic_translation` is a no-op).
        let restored = PlayerPose {
            position: [100.0, 50.0, -25.0],
            yaw: 0.5,
            pitch: 0.1,
            character_mode: true,
        };
        apply_player_pose(&mut world, &restored);
        let tq = world.query::<Transform>().unwrap();
        assert_eq!(tq.get(body).unwrap().translation, Vec3::new(100.0, 50.0, -25.0));
    }

    /// A `PlayerPose` rides along in the snapshot as a registered resource
    /// and decodes back out by name — the wire the live load reads.
    #[test]
    fn player_pose_survives_snapshot_round_trip() {
        let reg = build_save_registry();
        let mut world = World::new();
        world.insert_resource(StringPool::new());
        world.insert_resource(FormIdPool::new());
        world.insert_resource(PlayerPose {
            position: [1.0, 2.0, 3.0],
            yaw: 0.7,
            pitch: -0.2,
            character_mode: true,
        });

        let snap = save_world(&world, &reg).unwrap();
        let bytes = encode(&snap, reg.schema_fingerprint()).unwrap();
        let decoded = decode(&bytes, reg.schema_fingerprint()).unwrap();

        let pose = snapshot_player_pose(&decoded).expect("pose column present");
        assert_eq!(pose.position, [1.0, 2.0, 3.0]);
        assert!(pose.character_mode);
        assert!((pose.yaw - 0.7).abs() < 1e-6);
    }
}
