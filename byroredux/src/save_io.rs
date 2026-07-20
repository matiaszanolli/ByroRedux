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
use byroredux_save::validate::{
    log_validation_warnings, validate_world, ValidationError, ValidationKind,
};
use byroredux_save::{disk, encode, save_world, SaveRegistry, Snapshot};

/// The **mutable game-state** component columns a live load overlays onto
/// a reloaded cell, keyed by stable form id. Deliberately excludes
/// structural/identity columns (`Name` / `Parent` / `Children` / the
/// form-id key) — the reloaded cell already owns those; only post-spawn
/// *changes* (moved objects, inventory, equip, light/script state) are
/// replayed.
///
/// `AnimationPlayer` / `AnimationStack` are also **excluded** (#1696): the
/// delta apply remaps each row's entity *key* (saved id → live id) but
/// moves the component *value* verbatim, and both carry session-local
/// references that the remap doesn't touch — `root_entity: Option<EntityId>`
/// (a saved-session id, meaningless in the freshly-reloaded cell) and
/// `clip_handle: u32` (an `AnimationClipRegistry` index, not stable across
/// a reload). Overlaying them clobbers the *correct* fresh `root_entity` the
/// cell loader just set (`scene/nif_loader.rs` re-spawns the player scoped to
/// the fresh subtree) with a stale one, breaking name-scoped channel lookups.
/// Their post-spawn playback state is transient, so letting the reloaded cell
/// own them wholesale is the right call. (A full restore — not a live overlay —
/// still round-trips them via the registry's `load` path.)
///
/// # Invariant — delta-safe fields only (SAVE-D1-02 / SAVE-D6-01)
///
/// Unlike the clear/restore path (`byroredux_save::restore_world`), the live
/// overlay never re-installs the saved `StringPool` and never rebuilds the
/// entity-id map for *values* — it reloads the cell (which owns its own pool +
/// freshly-spawned entities) and overlays component values **verbatim**. A
/// column may therefore carry **only session-stable fields**:
/// - **No [`FixedString`](byroredux_core::string::FixedString)** (or anything
///   `#[serde(with = "fixed_string_serde")]`): it serialises as a raw `u32`
///   symbol that means nothing in the reloaded cell's pool — silent string
///   corruption. (`Name` is excluded for exactly this reason.)
/// - **No `EntityId`** (or `Option<EntityId>` / `Vec<EntityId>`): a saved-session
///   id, meaningless after the cell respawns — this is why `AnimationPlayer`'s
///   `root_entity` keeps the pair off the list (SAVE-D6-01).
/// - **No session-local handles** (registry indices like `clip_handle`) that
///   aren't stable across a reload.
///
/// Pool-relative indices are fine *iff* their backing pool is itself a restored
/// save resource — e.g. `ItemStack.instance` (an `ItemInstancePool` index) is
/// safe because `ItemInstancePool` round-trips as a resource.
///
/// `delta_columns_carry_only_session_stable_fields` (a tripwire test below)
/// pins the exact set so any addition forces a maintainer through this checklist.
const MUTABLE_DELTA_COLUMNS: &[&str] = &[
    "Transform",
    "Inventory",
    "EquipmentSlots",
    "LightSource",
    "LightFlicker",
    "ScriptTimer",
    // #1834 — runtime-mutated by the `setav`/`modav` console commands
    // (`commands/actor_value.rs`). Delta-safe: the map is keyed by
    // global-space AVIF FormID (u32, stable across reload) with four `f32`
    // composition layers — no FixedString / EntityId / session handle.
    "ActorValues",
    // #2014 / SAVE-D1-NEW-01 — delta-safe subset of the seven M42
    // AI-procedure runtime-state components: WanderState/TravelState/
    // GuardState/PatrolState are plain Vec3/enum/u32 fields, and
    // Traveled/Escorted are empty terminal markers. FollowState/
    // EscortState/Seated are deliberately NOT here — they carry
    // `EntityId` fields (`target_entity`/`furniture`), the same
    // session-local-reference hazard `#1696` already excluded
    // `AnimationPlayer.root_entity` for. Those three still ride the full
    // register_component round-trip above, just not the live overlay.
    "WanderState",
    "TravelState",
    "Traveled",
    "GuardState",
    "PatrolState",
    "Escorted",
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
        // SAVE-D3-02 — resume the ring cursor past the newest on-disk slot so
        // the first quicksave after a restart doesn't clobber the most-recent
        // save (the cursor is in-memory and would otherwise restart at 0).
        let ring = disk::SaveRing::resume(ring_size, &dir);
        Self { dir, ring }
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
        ActorValues, Children, EquipmentSlots, EscortState, Escorted, FollowState, GuardState,
        Inventory, LightFlicker, LightSource, Name, Parent, PatrolState, Seated, Transform,
        TravelState, Traveled, WanderState,
    };
    use byroredux_core::ecs::resources::ItemInstancePool;
    use byroredux_scripting::quest_stages::{QuestObjectiveState, QuestStageState};
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
        // #1834 — layered actor values (SPECIAL / skills / resistances /
        // resources / derived). Stamped at NPC spawn from class auto-calc and
        // mutated live by `setav`/`modav`; pre-fix a save/load silently
        // reverted every edited/permanent/temporary/damage layer to the
        // re-derived spawn base. Also a MUTABLE_DELTA_COLUMN (delta-safe).
        .register_component::<ActorValues>("ActorValues")
        // #2014 / SAVE-D1-NEW-01 — the seven M42 AI-procedure runtime-state
        // components. Continuously-updated state (WanderState/PatrolState/
        // GuardState) is cosmetically self-correcting if lost (the owning
        // system re-derives `home`/`anchor` from the actor's post-reload
        // position on its next tick), but the terminal one-shot markers
        // (Traveled/Escorted/Seated) are not: losing them makes an
        // already-finished NPC silently redo its Travel/Escort/Seat
        // behavior. All nine ride full register_component (restore_world
        // preserves entity ids verbatim); see MUTABLE_DELTA_COLUMNS below
        // for which additionally get the live-overlay fast path.
        .register_component::<WanderState>("WanderState")
        .register_component::<TravelState>("TravelState")
        .register_component::<Traveled>("Traveled")
        .register_component::<FollowState>("FollowState")
        .register_component::<EscortState>("EscortState")
        .register_component::<Escorted>("Escorted")
        .register_component::<GuardState>("GuardState")
        .register_component::<PatrolState>("PatrolState")
        .register_component::<Seated>("Seated")
        .register_form_id_component("FormIdComponent")
        .register_resource::<ItemInstancePool>("ItemInstancePool")
        // M45.1 — the cell identity + plugin set the save was taken in,
        // so `load` knows which cell to reload before applying deltas.
        .register_resource::<CurrentCellContext>("CurrentCellContext")
        // M45.1 refinement — where the player was standing + looking, so
        // `load` restores the pose instead of the cell's default spawn.
        .register_resource::<PlayerPose>("PlayerPose")
        // #1862 / SAVE-07 — quest stage/objective progress is live gameplay
        // state (Papyrus `SetStage`/`GetStage`/`GetStageDone` and
        // `SetObjectiveDisplayed`/`SetObjectiveCompleted`/`SetObjectiveFailed`),
        // mutated every frame by real recognizer-emitted scripts
        // (quest_advance, dlc2_ttr4a, mg07_door). Pre-fix it silently reverted
        // to default on every save/load.
        .register_resource::<QuestStageState>("QuestStageState")
        .register_resource::<QuestObjectiveState>("QuestObjectiveState");
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

    // #2018 / SAVE-D6-03 — drive the body whenever the LIVE session is in
    // Character mode, regardless of which mode the pose was SAVED in.
    // Pre-fix this gated on `pose.character_mode && character_now`, so a
    // FlyCam-saved pose reloaded into a live Character session fell to the
    // camera-only branch below, leaving the body untouched;
    // `camera_follow_system` (Stage::Late, every frame while
    // `PlayerMode::Character`) unconditionally re-derives the camera
    // position from the body's `GlobalTransform` + eye height with no
    // awareness a pose was just restored, so the camera-only fix was
    // visible for exactly one frame before being silently overwritten —
    // same mechanism as the door-transition case `#1874` fixed.
    if character_now {
        if let Some(body) = body {
            // `pos` is the saved *camera* position when the pose was
            // captured in FlyCam mode (see `capture_player_pose`'s
            // `target` selection) — convert it to the body's feet position
            // the same way `snap_character_body_to_camera` does
            // (`cam_pos - eye_height` on Y), so `camera_follow_system`
            // re-derives the identical restored vantage every subsequent
            // frame instead of `body_pos.y + eye_height` landing one
            // `eye_height` too high. A Character-saved pose already IS the
            // body position, so it's used verbatim.
            let body_pos = if pose.character_mode {
                pos
            } else {
                let eye_height = world
                    .query::<byroredux_physics::CharacterController>()
                    .and_then(|q| q.get(body).map(|c| c.eye_height))
                    .unwrap_or(52.0);
                pos - Vec3::Y * eye_height
            };
            if let Some(mut tq) = world.query_mut::<Transform>() {
                if let Some(t) = tq.get_mut(body) {
                    t.translation = body_pos;
                }
            }
            if let Some(mut gq) = world.query_mut::<GlobalTransform>() {
                if let Some(g) = gq.get_mut(body) {
                    g.translation = body_pos;
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
            byroredux_physics::set_kinematic_translation(world, body, body_pos);
            return;
        }
    }

    // FlyCam, or Character-saved with no live body: drop the camera at the
    // saved spot with a yaw/pitch-derived rotation.
    let rot = Quat::from_rotation_y(pose.yaw) * Quat::from_rotation_x(pose.pitch);
    crate::cell_loader::reposition_camera(world, pos, rot);
}

/// `save [slot]` — validate, snapshot, and atomically write the world.
/// Binary-side supplement to [`validate_world`]: every `FormIdComponent`'s
/// session-local `FormId` handle must resolve to its stable `FormIdPair`
/// through the live [`FormIdPool`].
///
/// The snapshot serializer resolves `FormId → FormIdPair` at save time and
/// **silently drops** any handle that doesn't resolve — the entity reloads
/// without its `FormIdComponent`, a lost cross-session reference (see
/// `byroredux_save::registry`). `validate_world`'s docstring defers this
/// cross-plugin check to the binary because the binary owns the
/// `FormIdPool`; running it before the write turns that silent drop into a
/// loud abort, the same defense-in-depth the core gates give. SAVE-D4-01.
fn validate_form_ids(world: &World) -> Vec<ValidationError> {
    use byroredux_core::ecs::components::FormIdComponent;
    use byroredux_core::form_id::FormIdPool;

    let mut errors = Vec::new();
    let Some(q) = world.query::<FormIdComponent>() else {
        return errors;
    };
    let pool = world.try_resource::<FormIdPool>();
    for (entity, comp) in q.iter() {
        let resolves = pool.as_ref().is_some_and(|p| p.resolve(comp.0).is_some());
        if !resolves {
            let detail = match pool.as_ref() {
                Some(_) => format!("FormId handle {:?} doesn't resolve in FormIdPool", comp.0),
                None => "carries a FormIdComponent but the world has no FormIdPool".to_string(),
            };
            errors.push(ValidationError {
                entity,
                kind: ValidationKind::FormId,
                detail,
            });
        }
    }
    errors
}

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

        // Explicit slot, or the ring's next slot — a quicksave only
        // actually *advances* the ring once validation passes and the
        // write is committed to, below (#2017 / SAVE-D4-NEW-01). `peek`
        // is non-mutating: it reports the slot `advance` would hand out
        // without consuming a rotation, so the round-robin invariant
        // ("next quicksave lands one slot after the last SUCCESSFUL
        // one") holds even across repeated validation-aborted attempts.
        let is_quicksave = args.trim().is_empty();
        let slot = if is_quicksave {
            state.ring.peek()
        } else {
            match args.trim().parse::<u32>() {
                Ok(n) => n,
                Err(_) => return CommandOutput::error(format!("invalid slot '{}'", args.trim())),
            }
        };

        // Pre-save validation — refuse to persist a broken world. Core
        // referential-integrity gates plus the binary-only FormId-pool
        // resolvability check (which needs the `FormIdPool` this crate owns).
        let mut issues = validate_world(world);
        issues.extend(validate_form_ids(world));
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

        // Validation passed and the write is about to proceed — now it's
        // safe to actually consume the ring rotation.
        if is_quicksave {
            state.ring.advance();
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
    // is running with) — matches the cell-transition path. #2039 /
    // PERF-D7-02: this rebuild discards the same warm BGSM/BGEM/CSG
    // caches `step_cell_transition`'s identical rebuild does — see the
    // caching design note on `App::step_cell_transition` in
    // `app_step.rs` for the shape a shared cache should take.
    let args = crate::cli_args::effective_args();
    let tex_provider = crate::asset_provider::build_texture_provider(&args);
    let mut mat_provider = crate::asset_provider::build_material_provider(&args);

    // SAVE-D6-02 — pre-flight the reload BEFORE the destructive teardown.
    // `unload_current_interior` + `drain_streaming_state` are irreversible;
    // if the reload then fails (missing/corrupt ESM, renamed/absent cell
    // editor id) the old `Err => return` left the engine in an empty world
    // with the player stranded in the void. Both of those failure modes
    // surface in `validate_cell_loadable` (parse + cell lookup, the same
    // non-destructive prefix `load_cell_with_masters` runs first), so we can
    // catch them here and KEEP the current cell instead. The on-disk save is
    // untouched either way; this just preserves the live session.
    if let Err(e) = crate::cell_loader::validate_cell_loadable(
        &cell_ctx.masters,
        &cell_ctx.esm_path,
        &cell_ctx.cell_editor_id,
    ) {
        log::error!(
            "save load ABORTED — cannot reload cell '{}'; keeping the current cell so the \
             session isn't stranded in an empty world (the on-disk save is intact; relaunch \
             to recover): {:#}",
            cell_ctx.cell_editor_id,
            e
        );
        return;
    }

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
            // Always called (not gated on `Some`) so a cell with no
            // `XCLL`/resolvable `LTMP` still gets the engine-default
            // interior fallback rather than a stale carry-over from
            // whatever cell was loaded before the load-apply (FNV-D1-01).
            crate::cell_loader::apply_interior_cell_lighting(world, r.lighting.as_ref());
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

    // #1844 / SAVE-01 — mirror the save path's `validate_world` +
    // `validate_form_ids` pairing (see `SaveCommand::execute` above) as a
    // post-load diagnostic. A save written before a given validation
    // rule existed, or a hand-edited-but-CRC-valid file, would otherwise
    // overlay a referentially broken world with no warning. Diagnostic
    // only — a load can't cleanly fall back to the previous cell.
    let mut issues = validate_world(world);
    issues.extend(validate_form_ids(world));
    log_validation_warnings(
        &format!("save load: cell '{}'", cell_ctx.cell_editor_id),
        &issues,
    );

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
    use byroredux_core::ecs::components::{LightFlicker, LightSource, Transform};
    use byroredux_core::form_id::FormIdPool;
    use byroredux_core::math::Vec3;
    use byroredux_core::string::StringPool;
    use byroredux_save::{decode, restore_world};
    use byroredux_scripting::ScriptTimer;

    /// Tripwire for the [`MUTABLE_DELTA_COLUMNS`] invariant (SAVE-D1-02 /
    /// SAVE-D6-01): the live overlay applies component *values* verbatim
    /// onto a reloaded cell without re-installing the saved `StringPool`
    /// or remapping value-embedded entity ids, so a delta column may carry
    /// only session-stable fields — **no `FixedString`, no `EntityId`, no
    /// session-local registry handle**.
    ///
    /// Rust has no field reflection, so this can't auto-scan the structs.
    /// Instead it pins the exact set against an audited expectation: adding
    /// a column makes this fail, forcing the maintainer to confirm the new
    /// type is delta-safe (per the doc comment) and update `AUDITED` here.
    /// `Name` (FixedString) and `AnimationPlayer`/`AnimationStack`
    /// (EntityId + registry handle) are the registered-but-excluded types
    /// this guard exists to keep out.
    #[test]
    fn delta_columns_carry_only_session_stable_fields() {
        // Each entry was hand-verified free of FixedString / EntityId /
        // session-handle fields (Transform: glam f32s; Inventory: u32 +
        // ItemInstancePool index; EquipmentSlots: Option<u32> array;
        // LightSource/LightFlicker: f32/u32 + [f32;3]; ScriptTimer: u32+f32).
        const AUDITED: &[&str] = &[
            "Transform",
            "Inventory",
            "EquipmentSlots",
            "LightSource",
            "LightFlicker",
            "ScriptTimer",
            // #1834 — ActorValues: HashMap<u32 AVIF-FormID, [f32; 4] layers>.
            // Keys are global-space FormIDs (stable across reload); values are
            // plain f32s. No FixedString / EntityId / session handle → delta-safe.
            "ActorValues",
            // #2014 — WanderState/PatrolState: home/target Vec3 + WanderPhase
            // enum (Walking, or Paused{remaining: f32}) + pick_count u32.
            // TravelState: destination Vec3. GuardState: anchor Vec3.
            // Traveled/Escorted: empty unit-struct terminal markers. None
            // carry FixedString / EntityId / session handle → delta-safe.
            "WanderState",
            "TravelState",
            "Traveled",
            "GuardState",
            "PatrolState",
            "Escorted",
        ];
        assert_eq!(
            MUTABLE_DELTA_COLUMNS, AUDITED,
            "MUTABLE_DELTA_COLUMNS changed: a delta column must carry no \
             FixedString / EntityId / session-handle field (see the type's \
             doc comment). If the new type is delta-safe, add it to AUDITED.",
        );
    }

    /// The binary's curated registry must round-trip its full type set —
    /// including the cross-crate `ScriptTimer`, a stable form id, and
    /// (SAVE-D2-04 / #2021) `LightSource`/`LightFlicker` — through
    /// encode → decode → restore into a fresh World.
    ///
    /// SIBLING sweep (#2021): the other registered types with an
    /// audit-confirmed flat shape (no `FixedString`/`EntityId`) —
    /// `Inventory`/`EquipmentSlots` (round-tripped by
    /// `crates/save/tests/round_trip.rs`'s `build_source_world`),
    /// `ActorValues` (`actor_values_survive_save_load_round_trip`), and
    /// `ScriptTimer` (this test) — already have dedicated round-trip
    /// coverage. `FollowState`/`EscortState`/`Seated` are excluded from
    /// this sweep: per the `MUTABLE_DELTA_COLUMNS` doc comment above they
    /// carry `EntityId` fields, so they're not flat and a gap there would
    /// be a different (higher-risk) finding, not this one.
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
        src.insert(
            e,
            LightSource {
                radius: 512.0,
                color: [0.9, 0.6, 0.2],
                flags: 0x0000_0008, // LIGHT_FLAG_FLICKER
                dimmer: 0.75,
                intensity: 1.25,
                falloff_exponent: 2.0,
            },
        );
        src.insert(
            e,
            LightFlicker {
                animation_flags: byroredux_core::ecs::LIGHT_FLAG_FLICKER,
                period_secs: 0.5,
                intensity_amplitude: 0.25,
                movement_amplitude: 1.5,
                base_translation: [10.0, 20.0, 30.0],
                phase_offset_secs: 0.125,
            },
        );

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

        let light = dst.query::<LightSource>().unwrap().get(0).copied().unwrap();
        assert_eq!(light.radius, 512.0);
        assert_eq!(light.color, [0.9, 0.6, 0.2]);
        assert_eq!(light.flags, 0x0000_0008);
        assert_eq!(light.dimmer, 0.75);
        assert_eq!(light.intensity, 1.25);
        assert_eq!(light.falloff_exponent, 2.0);

        let flicker = dst.query::<LightFlicker>().unwrap().get(0).copied().unwrap();
        assert_eq!(flicker.period_secs, 0.5);
        assert_eq!(flicker.intensity_amplitude, 0.25);
        assert_eq!(flicker.movement_amplitude, 1.5);
        assert_eq!(flicker.base_translation, [10.0, 20.0, 30.0]);
        assert_eq!(flicker.phase_offset_secs, 0.125);
    }

    /// #1834 — an actor's layered `ActorValues` (class-auto-calc base plus any
    /// `setav`/`modav` console edit) must survive a save → load round-trip.
    /// Pre-fix the component was neither registered nor serialised, so a
    /// reload dropped every permanent/temporary/damage layer and re-derived
    /// only the spawn base.
    #[test]
    fn actor_values_survive_save_load_round_trip() {
        use byroredux_core::ecs::components::ActorValues;
        const AV_HEALTH: u32 = 0x0000_02C9;
        let reg = build_save_registry();

        let mut src = World::new();
        src.insert_resource(StringPool::new());
        src.insert_resource(FormIdPool::new());
        let e = src.spawn();
        let mut av = ActorValues::new();
        av.set_base(AV_HEALTH, 100.0); // class auto-calc base
        av.mod_permanent(AV_HEALTH, 25.0); // e.g. a `modav` edit
        av.apply_damage(AV_HEALTH, 40.0);
        src.insert(e, av);

        let snap = save_world(&src, &reg).unwrap();
        let bytes = encode(&snap, reg.schema_fingerprint()).unwrap();
        let decoded = decode(&bytes, reg.schema_fingerprint()).unwrap();

        let mut dst = World::new();
        dst.insert_resource(FormIdPool::new());
        restore_world(&mut dst, &reg, &decoded).unwrap();

        let q = dst.query::<ActorValues>().unwrap();
        let (_, restored) = q.iter().next().expect("ActorValues must round-trip");
        // All four layers survive (pre-#1834 the whole component was dropped).
        assert_eq!(restored.current(AV_HEALTH), 85.0, "100 + 25 − 40");
        let layer = restored.get(AV_HEALTH).expect("entry present after reload");
        assert_eq!(layer.base, 100.0);
        assert_eq!(layer.permanent_mod, 25.0);
        assert_eq!(layer.damage, 40.0);
    }

    /// #2014 / SAVE-D1-NEW-01 — the seven M42 AI-procedure runtime-state
    /// components must survive a save → load round-trip. Pre-fix, none were
    /// registered, so a reload silently dropped them — the sharpest edge
    /// being a terminal one-shot marker like `Traveled`: an NPC that had
    /// already finished its Travel behavior would come back unfinished and
    /// silently redo it. Covers one delta-safe state type (`WanderState`),
    /// one terminal marker (`Traveled`), and one `EntityId`-carrying type
    /// (`Seated`) — the three distinct save shapes this fix introduces.
    #[test]
    fn ai_procedure_state_and_terminal_markers_survive_save_load_round_trip() {
        use byroredux_core::ecs::components::{Seated, Traveled, WanderPhase, WanderState};
        let reg = build_save_registry();

        let mut src = World::new();
        src.insert_resource(StringPool::new());
        src.insert_resource(FormIdPool::new());

        let wanderer = src.spawn();
        src.insert(
            wanderer,
            WanderState {
                home: Vec3::new(1.0, 2.0, 3.0),
                target: Vec3::new(4.0, 5.0, 6.0),
                phase: WanderPhase::Paused { remaining: 2.5 },
                pick_count: 7,
            },
        );

        let arrived = src.spawn();
        src.insert(arrived, Traveled);

        let furniture = src.spawn();
        let sitter = src.spawn();
        src.insert(sitter, Seated { furniture });

        let snap = save_world(&src, &reg).unwrap();
        let bytes = encode(&snap, reg.schema_fingerprint()).unwrap();
        let decoded = decode(&bytes, reg.schema_fingerprint()).unwrap();

        let mut dst = World::new();
        dst.insert_resource(FormIdPool::new());
        restore_world(&mut dst, &reg, &decoded).unwrap();

        let wq = dst.query::<WanderState>().unwrap();
        let (_, restored_wander) = wq.iter().next().expect("WanderState must round-trip");
        assert_eq!(restored_wander.home, Vec3::new(1.0, 2.0, 3.0));
        assert_eq!(restored_wander.target, Vec3::new(4.0, 5.0, 6.0));
        assert_eq!(restored_wander.phase, WanderPhase::Paused { remaining: 2.5 });
        assert_eq!(restored_wander.pick_count, 7);

        let tq = dst.query::<Traveled>().unwrap();
        assert_eq!(
            tq.iter().count(),
            1,
            "Traveled must round-trip — losing it makes an already-arrived NPC redo its Travel"
        );

        let sq = dst.query::<Seated>().unwrap();
        let (_, restored_seated) = sq.iter().next().expect("Seated must round-trip");
        // restore_world preserves entity ids verbatim, so the furniture
        // reference survives even though it wasn't a MUTABLE_DELTA_COLUMN.
        assert_eq!(restored_seated.furniture, furniture);
    }

    /// #1835 — every gameplay-state component `spawn_npc_entity` stamps on an
    /// NPC placement root must be a deliberate save decision: registered in
    /// [`build_save_registry`] (persisted + restored) XOR listed as
    /// re-derived-from-static-ESM-at-respawn (write-once, no runtime mutator,
    /// so not saving it is a correct no-op). A new spawn-stamp that is neither
    /// — or one wrongly in both — trips this test. This is the structural
    /// guard the `ActorValues` (#1834) gap lacked, so the pattern can't
    /// silently repeat a third time.
    ///
    /// Manually maintained — Rust has no reflection over `world.insert` sites,
    /// same tripwire philosophy as `delta_columns_carry_only_session_stable_fields`.
    /// When a runtime mutator lands for a re-derived type (leveling XP,
    /// `AddPerk`, a faction-rank command), register it AND drop it from the
    /// allowlist in the SAME commit (per #1835).
    #[test]
    fn npc_spawn_stamped_components_are_saved_or_intentionally_rederived() {
        // Persistent gameplay-state components stamped on the placement root by
        // `spawn_npc_entity` + its `stamp_*` helpers (`npc_spawn.rs`). Pure
        // placement scaffolding (Parent/Children), GPU handles, and transient
        // markers are out of scope — this guards actor state, the #1834 class.
        const NPC_SPAWN_STAMPED: &[&str] = &[
            "Transform",
            "Name",
            "Inventory",
            "EquipmentSlots",
            "ActorValues",
            "FactionRanks",
            "CharacterLevel",
            "Background",
            "Perks",
        ];
        // Write-once from static ESM `NPC_` data — no runtime mutator exists,
        // so a save/load re-derives byte-identical values on respawn and NOT
        // saving them is a correct no-op (#1835). Register + remove from here
        // the moment a mutator lands.
        const REDERIVED_NOT_SAVED: &[&str] =
            &["FactionRanks", "CharacterLevel", "Background", "Perks"];

        let registered: std::collections::HashSet<&str> =
            build_save_registry().component_names().collect();

        for name in NPC_SPAWN_STAMPED {
            let saved = registered.contains(name);
            let rederived = REDERIVED_NOT_SAVED.contains(name);
            assert!(
                saved ^ rederived,
                "NPC-spawn-stamped {name:?}: must be EITHER registered in \
                 build_save_registry (saved={saved}) OR in REDERIVED_NOT_SAVED \
                 (rederived={rederived}) — never both/neither. If it gained a \
                 runtime mutator, register it (#1834); if it stays write-once \
                 from ESM, allowlist it (#1835).",
            );
        }
    }

    /// A clean validation pass is the precondition every save checks.
    #[test]
    fn fresh_world_validates_clean() {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, Transform::default());
        assert!(validate_world(&world).is_empty());
    }

    /// SAVE-D4-01 (SIBLING): a `FormIdComponent` whose handle doesn't
    /// resolve in the live `FormIdPool` is rejected by the binary-side gate
    /// before the write — otherwise the snapshot serializer silently drops
    /// it and the entity reloads without its form id.
    #[test]
    fn unresolvable_form_id_is_rejected() {
        use byroredux_core::ecs::components::FormIdComponent;
        use byroredux_core::form_id::{FormIdPair, LocalFormId, PluginId};

        let mut world = World::new();
        world.insert_resource(FormIdPool::new()); // empty — resolves nothing

        // Mint a handle in a throwaway pool; the world's empty pool can't
        // resolve it (an empty `to_pair` yields `None` for any index).
        let stray = {
            let mut tmp = FormIdPool::new();
            tmp.intern(FormIdPair {
                plugin: PluginId::from_filename("Test.esm"),
                local: LocalFormId(0x07),
            })
        };

        let e = world.spawn();
        world.insert(e, FormIdComponent(stray));

        let errors = validate_form_ids(&world);
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert_eq!(errors[0].kind, ValidationKind::FormId);
        assert_eq!(errors[0].entity, e);
    }

    /// A resolvable handle (interned in the world's own pool) passes clean.
    #[test]
    fn resolvable_form_id_passes() {
        use byroredux_core::ecs::components::FormIdComponent;
        use byroredux_core::form_id::{FormIdPair, LocalFormId, PluginId};

        let mut world = World::new();
        world.insert_resource(FormIdPool::new());
        let fid = {
            let mut pool = world.resource_mut::<FormIdPool>();
            pool.intern(FormIdPair {
                plugin: PluginId::from_filename("Test.esm"),
                local: LocalFormId(0x07),
            })
        };
        let e = world.spawn();
        world.insert(e, FormIdComponent(fid));
        assert!(validate_form_ids(&world).is_empty());
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

    /// #2017 / SAVE-D4-NEW-01 — a quicksave (blank-slot `save`) whose
    /// pre-save validation aborts must NOT consume a ring rotation. Pre-fix
    /// `state.ring.advance()` ran before the validation gate, so a failed
    /// attempt permanently burned a slot with nothing written to back it,
    /// breaking "next quicksave lands one slot after the last SUCCESSFUL
    /// one". Drives one aborted attempt (world carries an unresolvable
    /// `FormIdComponent`, mirroring `unresolvable_form_id_is_rejected`)
    /// followed by one successful attempt, and checks the ring cursor only
    /// moved on the successful write.
    #[test]
    fn quicksave_ring_cursor_does_not_advance_on_validation_abort() {
        use byroredux_core::ecs::components::FormIdComponent;
        use byroredux_core::form_id::{FormIdPair, LocalFormId, PluginId};

        let mut world = World::new();
        world.insert_resource(StringPool::new());
        world.insert_resource(FormIdPool::new());
        world.insert_resource(build_save_registry());
        let dir = std::env::temp_dir().join(format!("byro_ring_abort_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        world.insert_resource(SaveState::new(dir.clone(), 4));

        // A stray FormId handle minted in a throwaway pool — the world's
        // own (empty) pool can't resolve it, so `validate_form_ids` fails
        // and `SaveCommand::execute` must abort without writing.
        let stray = {
            let mut tmp = FormIdPool::new();
            tmp.intern(FormIdPair {
                plugin: PluginId::from_filename("Test.esm"),
                local: LocalFormId(0x07),
            })
        };
        let bad_entity = world.spawn();
        world.insert(bad_entity, FormIdComponent(stray));

        assert_eq!(
            world.resource::<SaveState>().ring.peek(),
            0,
            "fresh ring starts at slot 0"
        );

        // Attempt 1: quicksave, world is invalid → must abort.
        let out = SaveCommand.execute(&world, "");
        assert!(
            out.lines.iter().any(|l| l.contains("ABORTED")),
            "invalid world must abort the save: {:?}",
            out.lines
        );
        assert_eq!(
            world.resource::<SaveState>().ring.peek(),
            0,
            "an aborted quicksave must NOT advance the ring cursor"
        );

        // Fix the world (drop the stray-handle entity) and retry.
        world.despawn(bad_entity);
        let out = SaveCommand.execute(&world, "");
        assert!(
            out.lines.iter().any(|l| l.contains("saved slot 0")),
            "valid world must save to the still-unconsumed slot 0: {:?}",
            out.lines
        );
        assert_eq!(
            world.resource::<SaveState>().ring.peek(),
            1,
            "a successful quicksave must advance the ring cursor exactly once"
        );

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

    /// #2018 / SAVE-D6-03 — a pose saved in FlyCam mode (`pose.position` is
    /// the CAMERA position) reloaded into a live Character-mode session
    /// must still relocate the BODY, not fall through to the camera-only
    /// branch (which `camera_follow_system` would silently overwrite one
    /// frame later). The body's feet land `eye_height` below the saved
    /// camera position, mirroring `snap_character_body_to_camera`'s
    /// `cam_pos - eye_height` — so `camera_follow_system` re-derives
    /// exactly the saved camera vantage (`body.y + eye_height ==
    /// saved_camera.y`) every subsequent frame instead of reverting.
    #[test]
    fn player_pose_flycam_saved_relocates_body_in_live_character_mode() {
        use crate::components::InputState;
        use crate::systems::{PlayerEntity, PlayerMode};
        use byroredux_core::ecs::{GlobalTransform, Transform};
        use byroredux_core::math::Quat;
        use byroredux_physics::CharacterController;

        let mut world = World::new();
        world.insert_resource(PlayerMode::Character);
        world.insert_resource(InputState::default());
        let body = world.spawn();
        // Body sits far from the saved pose before restore — proves the
        // fallback branch (which left it untouched pre-fix) didn't run.
        world.insert(body, Transform::from_translation(Vec3::new(0.0, 0.0, 0.0)));
        world.insert(
            body,
            GlobalTransform::new(Vec3::new(0.0, 0.0, 0.0), Quat::IDENTITY, 1.0),
        );
        world.insert(body, CharacterController::HUMAN);
        world.insert_resource(PlayerEntity(Some(body)));

        // A FlyCam-mode save: `position` is the camera's absolute position.
        let saved = PlayerPose {
            position: [10.0, 200.0, 30.0],
            yaw: 0.3,
            pitch: -0.1,
            character_mode: false,
        };
        apply_player_pose(&mut world, &saved);

        let expected_body_y = 200.0 - CharacterController::HUMAN.eye_height;
        let tq = world.query::<Transform>().unwrap();
        let body_pos = tq.get(body).unwrap().translation;
        assert_eq!(
            body_pos,
            Vec3::new(10.0, expected_body_y, 30.0),
            "body must relocate to camera_pos - eye_height, not stay untouched"
        );

        // `camera_follow_system`'s derivation must reproduce the exact
        // saved camera Y from this body placement.
        assert_eq!(
            body_pos.y + CharacterController::HUMAN.eye_height,
            200.0,
            "camera_follow_system's body.y + eye_height must reproduce the saved camera Y"
        );
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

    /// #1862 / SAVE-07 — `QuestStageState` and `QuestObjectiveState` are
    /// live gameplay state (Papyrus `SetStage`/`GetStageDone` and
    /// `SetObjectiveDisplayed`/`SetObjectiveCompleted`/`SetObjectiveFailed`),
    /// mutated every frame by real recognizer-emitted scripts. Pre-fix
    /// neither type carried a `Serialize`/`Deserialize` derive and neither
    /// was registered in `build_save_registry`, so both silently reverted
    /// to default on every save/load. This pins the round trip through the
    /// same snapshot → encode → decode → restore_resources pipeline the
    /// live M45.1 overlay load uses.
    #[test]
    fn quest_stage_and_objective_state_survive_snapshot_round_trip() {
        use byroredux_scripting::quest_stages::{QuestFormId, QuestObjectiveState, QuestStageState};

        let reg = build_save_registry();
        let mut world = World::new();
        world.insert_resource(StringPool::new());
        world.insert_resource(FormIdPool::new());

        let quest = QuestFormId(0x0002_2f08); // the real DA10 quest FormID
        let mut stages = QuestStageState::default();
        stages.set_stage(quest, 37);
        stages.set_stage(quest, 40);
        world.insert_resource(stages);

        let mut objectives = QuestObjectiveState::default();
        objectives.set_displayed(quest, 10, true);
        objectives.set_completed(quest, 10, true);
        world.insert_resource(objectives);

        let snap = save_world(&world, &reg).unwrap();
        let bytes = encode(&snap, reg.schema_fingerprint()).unwrap();
        let decoded = decode(&bytes, reg.schema_fingerprint()).unwrap();

        // Full restore_world path (loose/test load).
        let mut restored_world = World::new();
        byroredux_save::restore_world(&mut restored_world, &reg, &decoded).unwrap();
        let restored_stages = restored_world.resource::<QuestStageState>();
        assert_eq!(restored_stages.get_stage(quest), 40);
        assert!(restored_stages.get_stage_done(quest, 37));
        assert!(restored_stages.get_stage_done(quest, 40));
        assert!(!restored_stages.get_stage_done(quest, 20), "never-visited stage stays false");

        let restored_objectives = restored_world.resource::<QuestObjectiveState>();
        let status = restored_objectives.get(quest, 10);
        assert!(status.displayed);
        assert!(status.completed);
        assert!(!status.failed);

        // Live M45.1 overlay path (restore_resources — resource-only, no
        // entity clear/respawn).
        let mut overlay_world = World::new();
        overlay_world.insert_resource(StringPool::new());
        overlay_world.insert_resource(FormIdPool::new());
        byroredux_save::restore_resources(&mut overlay_world, &reg, &decoded).unwrap();
        let overlay_stages = overlay_world.resource::<QuestStageState>();
        assert_eq!(overlay_stages.get_stage(quest), 40);
        assert!(overlay_stages.get_stage_done(quest, 37));
    }

    /// Source files that define the save-participating types registered in
    /// [`build_save_registry`] — top-level columns AND the types nested
    /// inside them (an `Inventory`'s `ItemStack`, an `AnimationStack`'s
    /// `AnimationLayer`, the `FormIdPair` behind the form-id key column, …).
    ///
    /// KEEP IN LOCKSTEP with `build_save_registry`: registering a new saved
    /// type (or nesting a new type inside a saved column) means adding its
    /// defining file here so the SAVE-D2-01 guard below scans it.
    /// Paths are relative to `CARGO_MANIFEST_DIR` (the `byroredux/` crate).
    const SAVE_TYPE_SOURCES: &[&str] = &[
        "../crates/core/src/ecs/packed.rs",               // Transform
        "../crates/core/src/ecs/components/name.rs",      // Name
        "../crates/core/src/ecs/components/hierarchy.rs", // Parent, Children
        "../crates/core/src/ecs/components/inventory.rs", // Inventory, EquipmentSlots, ItemStack, InventoryIndex
        "../crates/core/src/ecs/components/light.rs",     // LightSource, LightFlicker
        "../crates/core/src/ecs/components/form_id.rs",   // FormIdComponent
        "../crates/core/src/ecs/components/actor_values.rs", // ActorValues
        "../crates/core/src/form_id.rs",                  // FormIdPair (the serialised key)
        "../crates/core/src/animation/player.rs",         // AnimationPlayer
        "../crates/core/src/animation/stack.rs",          // AnimationStack, AnimationLayer
        "../crates/core/src/ecs/resources/mod.rs",        // ItemInstancePool, ItemInstance
        "../crates/scripting/src/timer.rs",               // ScriptTimer
        "../crates/scripting/src/quest_stages.rs",        // QuestStageState, QuestObjectiveState + nested types
        "src/cell_loader/transition.rs",                  // CurrentCellContext
        "src/save_io.rs",                                 // PlayerPose
        "../crates/core/src/ecs/components/wander.rs",    // WanderState (+ WanderBehavior, WanderPhase)
        "../crates/core/src/ecs/components/travel.rs",    // TravelState, Traveled (+ TravelBehavior)
        "../crates/core/src/ecs/components/follow.rs",    // FollowState (+ FollowBehavior)
        "../crates/core/src/ecs/components/escort.rs",    // EscortState, Escorted (+ EscortBehavior)
        "../crates/core/src/ecs/components/guard.rs",     // GuardState (+ GuardBehavior)
        "../crates/core/src/ecs/components/patrol.rs",    // PatrolState (+ PatrolBehavior)
        "../crates/core/src/ecs/components/sandbox.rs",   // Seated (+ SandboxBehavior)
    ];

    /// SAVE-D2-01 (#1714) — a save-participating struct must not gain a
    /// `#[serde(default)]` field without a [`FORMAT_MAJOR`] bump.
    ///
    /// `schema_fingerprint` hashes column *type keys*, not field layout, so
    /// an intra-type field change slips past it. serde's required-field
    /// backstop only rejects an old save when the new field is *required*; a
    /// `#[serde(default)]` field default-fills a missing column entry on an
    /// old save, loading it **silently downgraded**. Until a versioned
    /// migrator chain exists, the only safe shape change is a `FORMAT_MAJOR`
    /// bump (which `decode` rejects across).
    ///
    /// This guard trips on the explicit-`#[serde(default)]` half of the
    /// footgun. The new-`Option` half can't be caught statically (legitimate
    /// `Option`s already exist in saved structs — e.g.
    /// `EquipmentSlots::occupants`, `AnimationStack::root_entity`); it rides
    /// the doc rule on [`byroredux_save::FORMAT_MAJOR`]. Static source scan,
    /// mirroring the `texture.rs` / `draw.rs` `include_str!` ordering checks.
    #[test]
    fn serde_default_on_saved_struct_requires_format_major_bump() {
        // Once a migrator chain governs evolution past v1, intra-type change
        // is handled by migration rather than this blanket ban — let it pass.
        if byroredux_save::FORMAT_MAJOR > 1 {
            return;
        }
        let manifest = env!("CARGO_MANIFEST_DIR");
        let mut offenders = Vec::new();
        for rel in SAVE_TYPE_SOURCES {
            let path = std::path::Path::new(manifest).join(rel);
            let src = std::fs::read_to_string(&path).unwrap_or_else(|e| {
                panic!(
                    "SAVE-D2-01 guard can't read {} ({e}); a save-participating \
                     type's file moved — update SAVE_TYPE_SOURCES.",
                    path.display()
                )
            });
            for (i, line) in src.lines().enumerate() {
                // Match the attribute form only (`#[serde(default …)]`), so a
                // comment / string mention of the attribute (this file has
                // several) doesn't self-trip the scan.
                if line.trim_start().starts_with("#[serde(default") {
                    offenders.push(format!("{rel}:{}", i + 1));
                }
            }
        }
        assert!(
            offenders.is_empty(),
            "SAVE-D2-01 (#1714): `#[serde(default)]` on a save-participating \
             struct masks an intra-type change at load — schema_fingerprint is \
             type-key-only, so an old save loads silently default-filled. Bump \
             byroredux_save::FORMAT_MAJOR (+ add a migrator) or drop the \
             default. Offenders: {offenders:?}",
        );
    }
}
