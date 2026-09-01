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
    log_validation_warnings, validate_entity_reference, validate_world, ValidationError,
    ValidationKind,
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
    // #2291 — default2StateActivator's live state and CTDA-visible numeric
    // VM variables contain only bools and stable u64 string hashes.
    "TwoStateActivator",
    "ScriptVariables",
    // #1834 — runtime-mutated by the `setav`/`modav` console commands
    // (`commands/actor_value.rs`). Delta-safe: the map is keyed by
    // global-space AVIF FormID (u32, stable across reload) with four `f32`
    // composition layers — no FixedString / EntityId / session handle.
    "ActorValues",
    // Combat state is session-stable: the weapon points into the saved
    // Inventory by u32 index and Dead is a zero-field lifecycle marker.
    "EquippedWeapon",
    "Dead",
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
    // #2292 / SAVE-D1-09 — `ActorControlState { restrained: bool }`. Single
    // bool, no session-local identity.
    "ActorControlState",
    // #3165 — the player entity outlives cell reload, so its mutable breath
    // and fractional drowning state must be overlaid explicitly. The pose
    // restore below still clears velocity/grounded/jump after this column.
    "CharacterController",
    // #2379 / SAVE-D1-14 — `RigidBodyData`: `MotionType` enum (no payload)
    // + 5 plain f32s (mass/friction/restitution/linear_damping/
    // angular_damping). No FixedString / EntityId / session handle.
    "RigidBodyData",
    // #2382 / SAVE-D1-17 — `RumbleOnActivate`: f32/bool property fields +
    // `RumbleState` enum (`Busy` carries one f32). No FixedString /
    // EntityId / session handle.
    "RumbleOnActivate",
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

#[derive(Default)]
pub struct SaveLoadNotifications(pub Vec<String>);

impl Resource for SaveLoadNotifications {}

/// A player-facing save/load request that must execute only after the frame's
/// scheduler has joined all parallel systems.
///
/// Input adapters (keyboard, native menus, and future SDK hosts) enqueue this
/// small value instead of entering the save registry's wide lock surface
/// directly. [`execute_pending_player_save_actions`] is the single executor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerSaveAction {
    Quicksave,
    Quickload,
}

impl PlayerSaveAction {
    pub(crate) fn context(self) -> &'static str {
        match self {
            Self::Quicksave => "player quicksave",
            Self::Quickload => "player quickload",
        }
    }
}

/// FIFO ingress for player save/load requests.
///
/// This is intentionally transient process state, not part of a save file.
#[derive(Default)]
pub struct PendingPlayerSaveActions(Vec<PlayerSaveAction>);

impl Resource for PendingPlayerSaveActions {}

/// Defer a player save/load request to the post-scheduler frame boundary.
///
/// Returning an error when boot has not installed the queue makes this usable
/// by embedders without hiding a partially initialized SDK/runtime.
pub fn queue_player_save_action(
    world: &World,
    action: PlayerSaveAction,
) -> Result<(), &'static str> {
    let Some(mut pending) = world.try_resource_mut::<PendingPlayerSaveActions>() else {
        return Err("player save-action queue not installed");
    };
    pending.0.push(action);
    Ok(())
}

fn notify_player(world: &World, message: impl Into<String>) {
    if let Some(mut notifications) = world.try_resource_mut::<SaveLoadNotifications>() {
        notifications.0.push(message.into());
    }
}

impl Resource for PlayerPose {}

/// Environment override for the save-slot root (#3009).
///
/// Mirrors `settings_io`'s `BYROREDUX_SETTINGS_PATH`. It exists so an
/// automated gate can exercise save → reload continuity without writing into
/// the operator's real quicksave ring: the default root is `<cwd>/saves`, and
/// the smoke harness runs the engine from the repository root, so a gate that
/// saved would otherwise litter the working tree and clobber whatever ring
/// slot the cursor happened to land on.
pub const SAVE_DIR_ENV: &str = "BYROREDUX_SAVE_DIR";

/// Resolve the save-slot root: [`SAVE_DIR_ENV`] when set and non-empty,
/// otherwise `<cwd>/saves`.
pub fn discover_save_dir() -> PathBuf {
    save_dir_from(std::env::var_os(SAVE_DIR_ENV))
}

/// [`discover_save_dir`] over an explicit override, so the empty-value and
/// default branches are testable without mutating the process environment
/// (which is shared by every test thread and cannot be done safely).
fn save_dir_from(override_value: Option<std::ffi::OsString>) -> PathBuf {
    match override_value.filter(|dir| !dir.is_empty()) {
        Some(dir) => PathBuf::from(dir),
        None => PathBuf::from("saves"),
    }
}

#[cfg(test)]
mod save_dir_tests {
    use super::{save_dir_from, SAVE_DIR_ENV};
    use std::ffi::OsString;
    use std::path::PathBuf;

    /// #3009 — the override exists so an automated save → reload gate does
    /// not write into the operator's real quicksave ring. An empty value must
    /// fall back rather than resolve to the process's current directory,
    /// which would put slots wherever the engine happened to be launched.
    #[test]
    fn the_save_dir_override_falls_back_on_absent_and_empty_values() {
        assert_eq!(save_dir_from(None), PathBuf::from("saves"));
        assert_eq!(save_dir_from(Some(OsString::new())), PathBuf::from("saves"));
        assert_eq!(
            save_dir_from(Some(OsString::from("/tmp/byro-smoke/saves"))),
            PathBuf::from("/tmp/byro-smoke/saves")
        );
        // The smoke gate spells this name; a rename here must break there.
        assert_eq!(SAVE_DIR_ENV, "BYROREDUX_SAVE_DIR");
    }
}

/// Where save slots live, plus the round-robin ring cursor.
///
/// Installed as a resource at startup. Default root is `<cwd>/saves`,
/// overridable with [`SAVE_DIR_ENV`].
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
///
/// Single-slot by design: two `load` commands in the same frame resolve
/// last-writer-wins. [`slot`](Self::slot) rides alongside so the
/// supersede can name *both* requests instead of dropping the earlier one
/// silently (#1848 / SAVE-05).
#[derive(Default)]
pub struct PendingSaveLoadSlot {
    /// Decoded snapshot awaiting the drain. `None` when idle.
    pub snapshot: Option<Snapshot>,
    /// Save-slot number [`snapshot`](Self::snapshot) was decoded from.
    /// Meaningless while `snapshot` is `None`.
    pub slot: u32,
}

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
        ActorValues, ActorVitals, Children, Dead, EquipmentSlots, EquippedWeapon, EscortState,
        Escorted, FollowState, GuardState, Inventory, LightFlicker, LightSource, Material, Name,
        Parent, PatrolState, RigidBodyData, Seated, Transform, TravelState, Traveled, WanderState,
    };
    use byroredux_core::ecs::resources::ItemInstancePool;
    use byroredux_scripting::papyrus_demo::RumbleOnActivate;
    use byroredux_scripting::quest_stages::{QuestObjectiveState, QuestStageState};
    use byroredux_scripting::{
        ActorCinematicState, ActorControlState, CinematicPresentationState, FragmentExecutionQueue,
        Globals, HorseTetherState, PapyrusProviderContinuationQueue, PlayerControlState,
        QuestAliasInjectionState, ReferenceEnableState, ScriptTimer, ScriptVariables,
        TwoStateActivator,
    };

    use crate::cell_loader::{CurrentCellContext, CurrentExteriorContext};
    use crate::components::GameTimeRes;

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
        // #2291 — levers, switches, and puzzle doors recognized as
        // default2StateActivator mutate both the state machine and its
        // CTDA-visible VM variables at runtime. Both are delta-safe.
        .register_component::<TwoStateActivator>("TwoStateActivator")
        .register_component::<ScriptVariables>("ScriptVariables")
        // #1834 — layered actor values (SPECIAL / skills / resistances /
        // resources / derived). Stamped at NPC spawn from class auto-calc and
        // mutated live by `setav`/`modav`; pre-fix a save/load silently
        // reverted every edited/permanent/temporary/damage layer to the
        // re-derived spawn base. Also a MUTABLE_DELTA_COLUMN (delta-safe).
        .register_component::<ActorValues>("ActorValues")
        // #3027 (SAVE-D1-2026-08-16-02) — registered (so a hand load of an
        // older save still resolves the column), but deliberately absent
        // from `MUTABLE_DELTA_COLUMNS` below: despite the name/field
        // reading like a live HP value, `ActorVitals.health` is a stable
        // per-game AVIF FormID key (see the type's own doc comment,
        // `crates/core/src/ecs/components/actor_values.rs`), stamped once
        // at NPC spawn from static game data and never mutated at runtime
        // — combat damage writes through `ActorValues` (already a delta
        // column) using this FormID as the lookup key, not through this
        // struct. Mid-session health changes already survive a reload via
        // that path; this is write-once/re-derivable, the same shape as
        // `CharacterLevel`/`Perks` below, just not listed in
        // `REDERIVED_NOT_SAVED` because it's plain-registered (not the
        // save-omitted case that allowlist tracks).
        .register_component::<ActorVitals>("ActorVitals")
        .register_component::<EquippedWeapon>("EquippedWeapon")
        .register_component::<Dead>("Dead")
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
        // #2380 / SAVE-D1-15 — MQ101 cinematic fragment-effect state. Both
        // carry `EntityId` fields (`vehicle`/`horse`), the same
        // session-local-reference hazard as `FollowState`/`EscortState`
        // above — full `register_component` only, never
        // `MUTABLE_DELTA_COLUMNS`. Live-mutated by Papyrus PlayIdle/
        // SetVehicle/TetherToHorse fragment effects with no reload
        // re-derivation (quest_fragment_dispatch_system is edge-triggered
        // and doesn't replay on load, disproving #2294's assumption for
        // these two).
        .register_component::<ActorCinematicState>("ActorCinematicState")
        .register_component::<HorseTetherState>("HorseTetherState")
        // #2292 / SAVE-D1-09 — `Actor.SetRestrained` per-actor lock. Plain
        // bool, delta-safe. Pre-fix a save taken while an NPC was restrained
        // silently freed it on reload.
        .register_component::<ActorControlState>("ActorControlState")
        .register_component::<byroredux_physics::CharacterController>("CharacterController")
        // #2379 / SAVE-D1-14 — `motion_type` (+ mass/friction/restitution/
        // damping) is mutated at runtime by Papyrus `.SetMotionType()`
        // (`scripted_motion_type_system`), not just static bhkRigidBody
        // import data. Flat enum + f32s, delta-safe. Pre-fix a scripted
        // motion-type change (e.g. making a prop dynamic for a scripted
        // sequence) silently reverted to the ESM-derived default on reload.
        .register_component::<RigidBodyData>("RigidBodyData")
        // #2378 / SAVE-D1-13 — the `mat.set` debug console command
        // live-mutates a Material's PBR scalars/colors/material_kind at
        // runtime, the same class of gap #1834 fixed for `ActorValues`
        // (also debug-command-mutated via `setav`/`modav`). Every field
        // is a plain scalar / `Option<String>` / nested plain-data struct
        // (no FixedString / EntityId), delta-safe for the full round-trip
        // registry. Deliberately NOT added to `MUTABLE_DELTA_COLUMNS`
        // below: unlike a per-actor state machine, Material sits on
        // nearly every rendered entity in a cell, so a live-load overlay
        // would replace every mesh's freshly-NIF-imported material with
        // whatever was captured at save time — a far bigger blast radius
        // than the narrow "one debug-edited entity" case this fixes.
        // `mat.set` edits still survive a full save/load (the primary
        // ask), just not the fast live-reload-and-overlay path.
        .register_component::<Material>("Material")
        // #2382 / SAVE-D1-17 — `defaultRumbleOnActivate`'s live
        // Active/Busy/Inactive state machine + wait countdown. Flat enum
        // + f32/bool fields, delta-safe. Pre-fix a mid-wait or
        // already-one-shot-fired lever silently reset to `Active` on
        // reload, letting a spent trap/lever re-fire.
        .register_component::<RumbleOnActivate>("RumbleOnActivate")
        .register_form_id_component("FormIdComponent")
        .register_resource::<ItemInstancePool>("ItemInstancePool")
        // M45.1 — the cell identity + plugin set the save was taken in,
        // so `load` knows which cell to reload before applying deltas.
        .register_resource::<CurrentCellContext>("CurrentCellContext")
        // EX-09/17 item 4 — the exterior-mode counterpart to
        // `CurrentCellContext`: worldspace/grid/radius identity for a save
        // taken mid-exterior-streaming, so `load` can rebuild the same
        // `WorldStreamingState` instead of rejecting exterior saves outright.
        .register_resource::<CurrentExteriorContext>("CurrentExteriorContext")
        // M45.1 refinement — where the player was standing + looking, so
        // `load` restores the pose instead of the cell's default spawn.
        .register_resource::<PlayerPose>("PlayerPose")
        // M34 day/night completion — the clock is mutable gameplay state:
        // weather advances it every frame and `time.*` controls can change
        // hour/rate/day. Restoring after the cell reload lets the next
        // weather tick re-derive sky, fog, sun, and directional lighting.
        .register_resource::<GameTimeRes>("GameTimeRes")
        // #1862 / SAVE-07 — quest stage/objective progress is live gameplay
        // state (Papyrus `SetStage`/`GetStage`/`GetStageDone` and
        // `SetObjectiveDisplayed`/`SetObjectiveCompleted`/`SetObjectiveFailed`),
        // mutated every frame by real recognizer-emitted scripts
        // (quest_advance, dlc2_ttr4a, mg07_door). Pre-fix it silently reverted
        // to default on every save/load.
        .register_resource::<QuestStageState>("QuestStageState")
        .register_resource::<QuestObjectiveState>("QuestObjectiveState")
        // MQ101's startup fragment writes GameHour before advancing to stage
        // 10. GLOB values are mutable game state, so a save/load must retain
        // the scripted value instead of silently restoring the ESM default.
        .register_resource::<Globals>("Globals")
        // QUST alias CNTO injections are permanent. Preserve their grant
        // ledger so the derived alias refresh after load cannot duplicate
        // already-saved inventory stacks. Faction bookkeeping inside this
        // resource is serde-skipped and re-derived from static QUST data.
        .register_resource::<QuestAliasInjectionState>("QuestAliasInjectionState")
        // #2292 / SAVE-D1-09 — `Game.EnablePlayerControls`/
        // `DisablePlayerControls` lock state. Plain bool/i32 fields,
        // delta-safe. Pre-fix a save taken mid-scripted-sequence (controls
        // locked) silently reset to all-enabled on reload.
        .register_resource::<PlayerControlState>("PlayerControlState")
        // #2381 / SAVE-D1-16 — suspended `Utility.Wait`/
        // `WaitForActors3DLoaded` fragment continuations. Every nested
        // type (`Effect`/`ActorRef`/`ObjectRef`/`QuestRef`/
        // `ScriptInstanceData` and friends) audited field-by-field for
        // FixedString/EntityId hazards — none found, all plain data
        // (FormIDs as u32, property names as owned String). Pre-fix a
        // save taken mid-`Utility.Wait` silently dropped the queued
        // effect chain the wait was gating.
        .register_resource::<FragmentExecutionQueue>("FragmentExecutionQueue")
        // Provider-bearing OnLoad/OnActivate/OnTriggerEnter/OnUpdate handlers
        // retain typed locals plus their lowered route/tail while suspended at
        // Utility.Wait. All nested identities are stable manifest strings;
        // no EntityId or process-local handles cross the save boundary.
        .register_resource::<PapyrusProviderContinuationQueue>("PapyrusProviderContinuationQueue")
        .register_resource::<ReferenceEnableState>("ReferenceEnableState")
        // #2380 / SAVE-D1-15 — MQ101 cinematic presentation state
        // (sitting rotation, animation-event registrations, active IMAD
        // applications). No `EntityId`/`FixedString` anywhere.
        // `image_space_modifier_catalog` is static ESM data that rides
        // along redundantly (see the type's own doc comment).
        .register_resource::<CinematicPresentationState>("CinematicPresentationState");
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

/// Pull the saved exterior-streaming context out of a decoded snapshot, if
/// present. Returns `None` for saves taken outside exterior streaming
/// (interior / loose-NIF modes never set `CurrentExteriorContext`) — the
/// exterior counterpart of [`snapshot_cell_context`], EX-09/17 item 4.
pub fn snapshot_exterior_context(
    snap: &byroredux_save::Snapshot,
) -> Option<crate::cell_loader::CurrentExteriorContext> {
    snap.resources
        .get("CurrentExteriorContext")
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

/// Binary-side supplement to [`validate_world`], mirroring
/// [`validate_form_ids`]'s pattern: `validate_world`'s doc comment scopes
/// `crates/save`'s checks to `byroredux-core` types only, deferring
/// anything needing another crate's components to the binary — the same
/// reason `FormId` resolution lives here instead of in `crates/save`.
///
/// #2535 / SAVE-D4-02 — `HorseTetherState.horse: EntityId` and
/// `ActorCinematicState.vehicle: Option<EntityId>`
/// (`byroredux_scripting::cinematic`) are both direct entity references,
/// invisible to every one of `validate_world`'s four reference-class
/// checks (`validate_hierarchy` only walks `Parent`/`Children`,
/// `validate_equipment` only `EquipmentSlots`↔`Inventory`,
/// `validate_animation` only `AnimationPlayer`,
/// `validate_inventory_instances` only `Inventory.items[].instance`). A
/// save with either field pointing at an id `>= next_entity` (e.g. a
/// tethered horse that despawned mid-session while the tether component
/// survived) previously passed `validate_world` cleanly with no
/// diagnostic anywhere in the pipeline.
fn validate_cinematic_entity_refs(world: &World) -> Vec<ValidationError> {
    use byroredux_scripting::cinematic::{ActorCinematicState, HorseTetherState};

    let mut errors = Vec::new();
    let next_entity = world.next_entity_id();

    if let Some(q) = world.query::<HorseTetherState>() {
        for (entity, tether) in q.iter() {
            validate_entity_reference(
                entity,
                "HorseTetherState.horse",
                tether.horse,
                next_entity,
                &mut errors,
            );
        }
    }

    if let Some(q) = world.query::<ActorCinematicState>() {
        for (entity, state) in q.iter() {
            if let Some(vehicle) = state.vehicle {
                validate_entity_reference(
                    entity,
                    "ActorCinematicState.vehicle",
                    vehicle,
                    next_entity,
                    &mut errors,
                );
            }
        }
    }

    errors
}

pub struct SaveCommand;

/// Execute a quicksave after the scheduler has quiesced. Player input must use
/// [`queue_player_save_action`] instead of calling this lock-wide operation.
pub(crate) fn quicksave(world: &World) -> CommandOutput {
    SaveCommand.execute(world, "")
}

impl ConsoleCommand for SaveCommand {
    fn name(&self) -> &str {
        "save"
    }
    fn description(&self) -> &str {
        "save [slot] — validate + snapshot the world to a slot (default: next ring slot)"
    }
    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        // `registry` (SaveRegistry) stays held through `save_world`/`encode`
        // below, alongside the ~26 component-storage + ~7 resource read
        // locks `save_world`/`validate_world`/`validate_form_ids` take —
        // the widest single-hold edge fan-out in the process. Production
        // callers are constrained to quiescent lanes: remote operator command
        // dispatch runs in the exclusive `DebugDrainSystem`; native-overlay
        // command dispatch and the player-action drain both run after
        // `Scheduler::run` has joined every parallel system. No live system
        // can therefore form the other half of an ABBA cycle against this
        // hold. New input/SDK integrations must enqueue through
        // `queue_player_save_action`; moving execution onto a live scheduler
        // lane needs this ordering re-derived. #3113 / #2154.
        let Some(registry) = world.try_resource::<SaveRegistry>() else {
            return CommandOutput::error("save registry not installed");
        };
        let Some(state) = world.try_resource::<SaveState>() else {
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
        // resolvability check (which needs the `FormIdPool` this crate owns)
        // and the cinematic EntityId-reference check (#2535 — needs
        // `byroredux-scripting` types `crates/save` doesn't depend on).
        let mut issues = validate_world(world);
        issues.extend(validate_form_ids(world));
        issues.extend(validate_cinematic_entity_refs(world));
        if !issues.is_empty() {
            let mut lines = vec![format!(
                "save ABORTED: {} referential-integrity issue(s) — refusing to write a poisoned save:",
                issues.len()
            )];
            for issue in issues.iter().take(20) {
                lines.push(format!(
                    "  [{:?}] entity {}: {}",
                    issue.kind, issue.entity, issue.detail
                ));
            }
            if issues.len() > 20 {
                lines.push(format!("  … and {} more", issues.len() - 20));
            }
            return CommandOutput::lines(lines);
        }

        // Nothing past this point needs access to
        // `SaveState` — only the directory, cheaply copied — so drop the
        // guard now rather than holding it across `save_world`'s ~30-storage
        // snapshot walk (SAVE-D3-02 / #2154).
        let dir = state.dir.clone();
        drop(state);

        let mut snapshot = match save_world(world, &registry) {
            Ok(s) => s,
            Err(e) => return CommandOutput::error(format!("snapshot failed: {e}")),
        };
        let extension_rows = match crate::extensions::capture_extension_state(world, &mut snapshot)
        {
            Ok(count) => count,
            Err(error) => {
                return CommandOutput::error(format!("extension-state snapshot failed: {error:#}"));
            }
        };
        let bytes = match encode(&snapshot, registry.schema_fingerprint()) {
            Ok(b) => b,
            Err(e) => return CommandOutput::error(format!("encode failed: {e}")),
        };
        match disk::write_slot(&dir, slot, &bytes) {
            Ok(path) => {
                // Consume quicksave rotation only after every validation,
                // snapshot/extension-state encode, and disk commit succeeds.
                // A rejected transient extension row must not skip a slot.
                if is_quicksave {
                    world.resource_mut::<SaveState>().ring.advance();
                }
                if let Err(error) = crate::extensions::queue_session_event(
                    world,
                    byroredux_sdk::event::SessionEvent {
                        phase: byroredux_sdk::event::SessionPhase::SaveComplete,
                        slot: Some(slot),
                    },
                ) {
                    log::warn!(
                        "save slot {slot} committed, but its extension lifecycle event was not queued: {error}"
                    );
                }
                CommandOutput::lines(vec![
                    format!("saved slot {slot} → {}", path.display()),
                    format!(
                        "  {} entities-worth of rows across {} component columns, {} resource(s); \
                     {extension_rows} extension row(s)",
                        snapshot.row_count(),
                        snapshot.components.len(),
                        snapshot.resources.len()
                    ),
                    format!(
                        "  {} bytes (next_entity={})",
                        bytes.len(),
                        snapshot.next_entity
                    ),
                ])
            }
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
                        if pose.character_mode {
                            "character"
                        } else {
                            "flycam"
                        },
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

/// Queue a verified slot for the between-frame live-load drain.
pub fn queue_load_slot(world: &World, slot: u32) -> CommandOutput {
    LoadCommand.execute(world, &slot.to_string())
}

/// Parse and queue the `--load` value through the same command path used by
/// F9, the pause menu, and the debug console.
pub fn queue_startup_load(world: &World, value: &str) -> CommandOutput {
    match value.parse::<u32>() {
        Ok(slot) => queue_load_slot(world, slot),
        Err(_) => CommandOutput::error(format!(
            "--load requires a numeric save slot, got '{value}'"
        )),
    }
}

pub(crate) fn command_output_is_failure(output: &CommandOutput) -> bool {
    output
        .lines
        .iter()
        .any(|line| line.starts_with("Error:") || line.starts_with("save ABORTED"))
}

/// Queue the newest on-disk slot, matching conventional quickload behavior.
pub(crate) fn quickload_latest(world: &World) -> CommandOutput {
    let slots = world
        .try_resource::<SaveState>()
        .map(|state| disk::slots_by_recency(&state.dir))
        .unwrap_or_default();
    if slots.is_empty() {
        return CommandOutput::error("no save slots available");
    }
    let mut rejected = Vec::new();
    for slot in slots {
        let mut output = queue_load_slot(world, slot);
        if !command_output_is_failure(&output) {
            if !rejected.is_empty() {
                rejected.push(format!("falling back to valid slot {slot}"));
                rejected.append(&mut output.lines);
                return CommandOutput::lines(rejected);
            }
            return output;
        }
        rejected.push(format!(
            "skipped invalid quickload slot {slot}: {}",
            output.lines.join(" | ")
        ));
    }
    rejected.push("Error: no decodable save slots available".to_string());
    CommandOutput::lines(rejected)
}

/// Execute queued player actions after the scheduler has joined.
///
/// The queue guard is dropped before either command starts, so it never joins
/// the save registry/component lock set. FIFO order makes a quicksave followed
/// by quickload deterministic within one frame.
pub(crate) fn execute_pending_player_save_actions(
    world: &World,
) -> Vec<(PlayerSaveAction, CommandOutput)> {
    let actions = {
        let Some(mut pending) = world.try_resource_mut::<PendingPlayerSaveActions>() else {
            return Vec::new();
        };
        std::mem::take(&mut pending.0)
    };
    actions
        .into_iter()
        .map(|action| {
            let output = match action {
                PlayerSaveAction::Quicksave => quicksave(world),
                PlayerSaveAction::Quickload => quickload_latest(world),
            };
            (action, output)
        })
        .collect()
}

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
        // EX-09/17 item 4 — a live load can reconstruct either kind of
        // session now; only reject a snapshot carrying neither.
        let destination_label = match (
            snapshot_cell_context(&snapshot),
            snapshot_exterior_context(&snapshot),
        ) {
            (Some(cell_ctx), _) => format!("cell {}", cell_ctx.cell_editor_id),
            (None, Some(ext_ctx)) => format!(
                "worldspace '{}' @ ({},{})",
                ext_ctx.worldspace_key, ext_ctx.grid.0, ext_ctx.grid.1
            ),
            (None, None) => {
                return CommandOutput::error(
                    "save has no cell or exterior context (loose save) — live load needs one",
                );
            }
        };

        // Queue for the between-frames drain (needs &mut World + renderer).
        match world.try_resource_mut::<PendingSaveLoadSlot>() {
            Some(mut pending) => {
                // #1848 / SAVE-05 — the pending slot is a single Option, so
                // a second `load` issued before `execute_pending_save_loads`
                // drains replaces the first. Last-writer-wins is the intended
                // semantics; what was wrong is that the discarded request
                // vanished without a trace. Report it on both channels: the
                // engine log (for a `--bench-hold` session) and the command
                // output (for the byro-dbg operator who typed it).
                let superseded = pending.snapshot.is_some().then_some(pending.slot);
                pending.snapshot = Some(snapshot);
                pending.slot = slot;
                let mut lines = Vec::new();
                if let Some(prev) = superseded {
                    let msg = format!(
                        "queued load of slot {prev} superseded by slot {slot} before drain"
                    );
                    log::info!("save load: {msg}");
                    lines.push(msg);
                }
                lines.push(format!(
                    "queued load of slot {slot} → {destination_label} (applies next frame)",
                ));
                CommandOutput::lines(lines)
            }
            None => CommandOutput::error("load slot not installed"),
        }
    }
}

/// What a `reload_*_session` helper reports back to
/// [`execute_pending_save_loads`]'s shared tail (delta restore/apply,
/// validation, pose restore) once the world is repopulated.
struct ReloadOutcome {
    /// Human-readable location, for the shared tail's log lines —
    /// `"cell 'FooInterior01'"` or `"worldspace 'tamriel' @ (3,-2)"`.
    location_label: String,
    /// What got (re)populated — `"42 entities"` for an interior cell,
    /// `"9 cells streaming"` for an exterior worldspace. Deliberately not a
    /// single `entity_count: usize`: exterior streaming doesn't track a
    /// per-cell entity count the way `load_cell_with_masters`'s
    /// `CellLoadResult` does, and counting live entities under every
    /// just-loaded `CellRoot` post-hoc for a log line isn't worth the
    /// query — "N cells" is the honest number this path actually has.
    count_label: String,
}

/// Reload the saved interior cell (SAVE-D6-02 preflight → teardown →
/// `load_cell_with_masters`). Returns `None` (having already logged why)
/// on any failure — the caller's job is just to bail without running the
/// shared restore/apply tail.
fn reload_interior_session(
    world: &mut World,
    ctx: &mut byroredux_renderer::VulkanContext,
    streaming: &mut Option<crate::streaming::WorldStreamingState>,
    args: &[String],
    cell_ctx: &crate::cell_loader::CurrentCellContext,
) -> Option<ReloadOutcome> {
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
        let message = format!(
            "save load ABORTED — cannot reload cell '{}'; keeping the current cell (on-disk save intact): {e:#}",
            cell_ctx.cell_editor_id
        );
        log::error!("{message}");
        notify_player(world, message);
        return None;
    }

    // Tear down whatever's loaded, then reload the saved cell fresh.
    if streaming.is_some() {
        crate::streaming_helpers::drain_streaming_state(world, ctx, streaming);
    }
    crate::cell_loader::unload_current_interior(world, ctx);

    let tex_provider = crate::asset_provider::build_texture_provider(args);
    let mut mat_provider = crate::asset_provider::build_material_provider(args);
    let result = crate::cell_loader::load_cell_with_masters(
        &cell_ctx.masters,
        &cell_ctx.esm_path,
        &cell_ctx.cell_editor_id,
        world,
        ctx,
        &tex_provider,
        Some(&mut mat_provider),
    );
    match result {
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
            Some(ReloadOutcome {
                location_label: format!("cell '{}'", cell_ctx.cell_editor_id),
                count_label: format!("{} entities", r.entity_count),
            })
        }
        Err(e) => {
            let message = format!(
                "save load: failed to reload cell '{}': {e:#}",
                cell_ctx.cell_editor_id
            );
            log::error!("{message}");
            notify_player(world, message);
            None
        }
    }
}

/// Reload the saved exterior worldspace/grid (EX-09/17 item 4). Same
/// preflight-before-teardown posture as [`reload_interior_session`]:
/// `build_exterior_world_context` is the exterior equivalent of
/// `validate_cell_loadable` (it's the same ESM-parse-and-resolve work,
/// just with no separate validate-only variant), so it runs first and the
/// already-built context is threaded straight into
/// `scene::assemble_exterior_streaming` on success rather than paying a
/// second parse the way the interior path's separate validate+load calls
/// do.
///
/// In-flight-streaming-worker handling (the design decision the plan
/// flagged): whatever the current session has in flight is unconditionally
/// drained/cancelled, not waited-on or resumed — the same posture
/// `drain_streaming_state` already uses at every other exterior teardown
/// boundary (cell transitions, this same function's interior branch). A
/// discarded in-flight cell payload just means that cell isn't in `World`
/// yet; the fresh `WorldStreamingState` rebuilt below re-requests it from
/// scratch around the saved grid, so nothing is lost, only re-fetched.
/// Whether it's safe to run the saved-delta overlay (`build_form_id_remap`
/// + `apply_deltas` in [`execute_pending_save_loads`]) after a `FullRadius`
/// exterior bootstrap — #3499. `exterior_reload_bootstrap_mode()`'s whole
/// point is guaranteeing every saved cell is resident first, via
/// `bootstrap_waiting`'s `!pending.is_empty()` loop condition; but that
/// loop's one non-`pending`-driven exit (the streaming worker thread dying
/// mid-bootstrap) can return with cells still pending. Applying the overlay
/// in that state would silently drop every saved row belonging to a cell
/// that never arrived.
fn exterior_reload_overlay_is_safe(pending_after_bootstrap: usize) -> bool {
    pending_after_bootstrap == 0
}

#[cfg(test)]
mod exterior_reload_overlay_tests {
    use super::exterior_reload_overlay_is_safe;

    /// The common case: `stream_initial_radius`'s wait loop drained
    /// `pending` normally, so the overlay is safe to run.
    #[test]
    fn zero_pending_is_safe() {
        assert!(exterior_reload_overlay_is_safe(0));
    }

    /// #3499's regression case: the worker-disconnect `break` can exit the
    /// wait loop with cells still pending — the overlay must be refused,
    /// not silently applied against an incomplete world.
    #[test]
    fn nonzero_pending_is_unsafe() {
        assert!(!exterior_reload_overlay_is_safe(1));
        assert!(!exterior_reload_overlay_is_safe(48));
    }
}

fn reload_exterior_session(
    world: &mut World,
    ctx: &mut byroredux_renderer::VulkanContext,
    streaming: &mut Option<crate::streaming::WorldStreamingState>,
    args: &[String],
    ext_ctx: &crate::cell_loader::CurrentExteriorContext,
) -> Option<ReloadOutcome> {
    let wctx = match crate::cell_loader::build_exterior_world_context(
        &ext_ctx.masters,
        &ext_ctx.esm_path,
        ext_ctx.grid.0,
        ext_ctx.grid.1,
        ext_ctx.radius_load,
        Some(&ext_ctx.worldspace_key),
    ) {
        Ok(wctx) => wctx,
        Err(e) => {
            let message = format!(
                "save load ABORTED — cannot rebuild worldspace '{}'; keeping the current session (on-disk save intact): {e:#}",
                ext_ctx.worldspace_key
            );
            log::error!("{message}");
            notify_player(world, message);
            return None;
        }
    };

    // EX-14/15 item C2 (#2369) — deliberately NOT wired in here, despite
    // `docs/engine/stream-boundary-state-continuity.md` §6 naming this
    // reload path as one of three candidate sites. Skipping the
    // persistent-CELL rebuild would mean skipping its ONLY restore path
    // too: a save load's correctness model is "always rebuild fresh from
    // ESM, then restore" — the full per-component registry round-trip for
    // most components, plus `MUTABLE_DELTA_COLUMNS`'s targeted overlay
    // (`Transform`, `WanderState`, `TravelState`, `Traveled`, etc., all
    // FormID-keyed) for the rest. A preserved LIVE root would never pass
    // through either restore step, so its entities would silently keep
    // whatever state the CURRENT session left them in instead of the
    // state actually recorded in the save file being loaded — a real
    // save-fidelity regression, not an optimization, the first time this
    // session's persistent-cell state has drifted from the loaded save's
    // (any save older than "the same instant", i.e. every real load).
    // Item C2's identity-skip is scoped to the genuine still-in-session,
    // still-live worldspace crossing (`step_cell_transition`'s Exterior
    // arm) where there is no save file whose recorded state must win.

    // Tear down whatever's loaded, then rebuild the saved worldspace fresh.
    if streaming.is_some() {
        crate::streaming_helpers::drain_streaming_state(world, ctx, streaming);
    }
    crate::cell_loader::unload_current_interior(world, ctx);

    let tex_provider = crate::asset_provider::build_texture_provider(args);
    let mat_provider = crate::asset_provider::build_material_provider(args);
    let (state, _cam_center) = crate::scene::assemble_exterior_streaming(
        world,
        ctx,
        wctx,
        tex_provider,
        mat_provider,
        ext_ctx.grid,
        ext_ctx.radius_load,
        exterior_reload_bootstrap_mode(),
        // See the comment above `wctx`'s teardown for why item C2's
        // identity-skip is deliberately not used on the save-load path.
        None,
    );
    let location_label = format!(
        "worldspace '{}' @ ({},{})",
        ext_ctx.worldspace_key, ext_ctx.grid.0, ext_ctx.grid.1
    );
    let count_label = format!(
        "{} cells streaming ({} pending)",
        state.loaded.len(),
        state.pending.len()
    );
    world.insert_resource(crate::cell_loader::LoadedPluginSet {
        masters: ext_ctx.masters.clone(),
        esm_path: ext_ctx.esm_path.clone(),
    });
    // Re-stamp the identity mirror — `assemble_exterior_streaming` doesn't
    // (only `begin_exterior_streaming` does, and this path can't reuse that
    // without paying the second ESM parse it exists to avoid), and
    // `drain_streaming_state` above just cleared the stale one.
    world.insert_resource(crate::cell_loader::CurrentExteriorContext {
        worldspace_key: ext_ctx.worldspace_key.clone(),
        esm_path: ext_ctx.esm_path.clone(),
        masters: ext_ctx.masters.clone(),
        grid: ext_ctx.grid,
        radius_load: state.radius_load,
        radius_unload: state.radius_unload,
    });
    // #3499 — `exterior_reload_bootstrap_mode()` is `FullRadius` specifically
    // so this function's caller can safely run `build_form_id_remap` +
    // `apply_deltas` against a fully-resident world: `bootstrap_waiting`
    // loops until `state.pending` drains. But `stream_initial_radius`'s wait
    // loop has one exit that isn't `pending`-driven — the streaming worker
    // thread disconnecting mid-bootstrap — and on that path `pending` is
    // still non-empty here. Applying the delta overlay anyway would
    // silently drop every saved row belonging to a cell that never
    // arrived: the exact #3280 mechanism, on a narrower trigger. The fresh
    // ESM-only world is committed either way (nothing left to roll back —
    // the old session was already drained above), so abort only the
    // overlay: return `None` so the caller's `let Some(ReloadOutcome {..})
    // = outcome else { return; }` skips `build_form_id_remap`/`apply_deltas`
    // and everything after, the same posture `validate_snapshot_types` /
    // `validate_cell_loadable` already take for their own failure modes.
    let pending_after_bootstrap = state.pending.len();
    *streaming = Some(state);
    ctx.signal_temporal_discontinuity(crate::streaming_helpers::SVGF_TAA_STREAMING_RECOVERY_FRAMES);
    if !exterior_reload_overlay_is_safe(pending_after_bootstrap) {
        let message = format!(
            "save load: worldspace '{}' reloaded from ESM, but the streaming worker \
             disconnected mid-bootstrap with {pending_after_bootstrap} cell(s) still pending — \
             skipping the saved-delta overlay rather than silently dropping rows for cells that \
             never arrived. The session is now on fresh ESM state for this worldspace with NO \
             save-state overlay applied; reload the save again once the streaming worker issue \
             is resolved.",
            ext_ctx.worldspace_key
        );
        log::error!("{message}");
        notify_player(world, message);
        return None;
    }
    Some(ReloadOutcome {
        location_label,
        count_label,
    })
}

/// Live-load delta application is synchronous and runs immediately after the
/// exterior reload returns. The full radius must therefore be resident before
/// `build_form_id_remap` scans the world; foreground-first would permanently
/// drop saved rows belonging to still-pending peripheral cells (#3280).
fn exterior_reload_bootstrap_mode() -> crate::scene::ExteriorBootstrapMode {
    crate::scene::ExteriorBootstrapMode::FullRadius
}

/// Drain a queued live-load: reload the saved cell or exterior worldspace
/// via the existing loaders (full GPU/physics/camera setup), restore saved
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
    let (snapshot, loaded_slot) = {
        let Some(mut slot) = world.try_resource_mut::<PendingSaveLoadSlot>() else {
            return;
        };
        match slot.snapshot.take() {
            Some(snapshot) => (snapshot, slot.slot),
            None => return,
        }
    };

    let registry = build_save_registry();

    // #3163 — every typed column is decodable from the snapshot alone.
    // Reject before the irreversible cell/streaming teardown so no serde
    // failure can leave a half-overlaid world.
    if let Err(e) = byroredux_save::validate_snapshot_types(&registry, &snapshot) {
        let message = format!(
            "save load ABORTED — snapshot columns failed typed preflight; keeping the current session: {e}"
        );
        log::error!("{message}");
        notify_player(world, message);
        return;
    }
    if let Err(error) = crate::extensions::preflight_extension_state(world, &snapshot) {
        let message = format!(
            "save load ABORTED — extension state failed preflight; keeping the current session: \
             {error:#}"
        );
        log::error!("{message}");
        notify_player(world, message);
        return;
    }

    // Build asset providers from the boot CLI args (same BSAs the engine
    // is running with) — matches the cell-transition path. #2039 /
    // PERF-D7-02: this rebuild discards the same warm BGSM/BGEM/CSG
    // caches `step_cell_transition`'s identical rebuild does — see the
    // caching design note on `App::step_cell_transition` in
    // `app_step.rs` for the shape a shared cache should take.
    let args = crate::cli_args::effective_args();

    // #3789 — restore saved resources BEFORE the reload, not only after it.
    //
    // `ReferenceEnableState` is the FormID-keyed ledger a Papyrus `Disable()`
    // writes to, and since #3278 it has a *spawn-time* consumer:
    // `cell_loader::spawn::placement_is_disabled` consults it per placed
    // REFR, before any mesh, collider or light. The reload below therefore
    // takes every spawn decision against whatever ledger the live session
    // happens to hold, and the saved one used to arrive seventeen lines
    // later — too late to matter, because `apply_deltas` is additive-only by
    // contract and can neither spawn nor despawn.
    //
    // On the most common load in the game — start the engine, load a save —
    // the live ledger is `ReferenceEnableState::default()`, i.e. everything
    // enabled, so every reference the save recorded as disabled came back
    // solid and interactive. Nothing logged it: from the loader's point of
    // view it correctly honoured the ledger it was shown. The symmetric
    // same-session case is a reference disabled *after* the save spawning
    // content-less for the whole of that cell's residency.
    //
    // Restoring here is also the right place for the preflight ordering the
    // block above establishes: a resource-restore failure now aborts BEFORE
    // the irreversible cell/streaming teardown rather than after it, so a
    // bad snapshot can no longer leave a torn-down world behind.
    //
    // The post-reload call is kept (see below) rather than replaced: it
    // re-asserts the saved values over anything the reload itself rebuilt
    // (`CurrentCellContext`, `PlayerPose`), which is what it was there for.
    // `restore_resources` is a straight per-resource overwrite from the
    // snapshot, so running it twice is idempotent.
    if let Err(e) = byroredux_save::restore_resources(world, &registry, &snapshot) {
        let message = format!(
            "save load ABORTED — resource restore failed before cell reload; \
             keeping the current session: {e}"
        );
        log::error!("{message}");
        notify_player(world, message);
        return;
    }

    let outcome = if let Some(cell_ctx) = snapshot_cell_context(&snapshot) {
        reload_interior_session(world, ctx, streaming, &args, &cell_ctx)
    } else if let Some(ext_ctx) = snapshot_exterior_context(&snapshot) {
        reload_exterior_session(world, ctx, streaming, &args, &ext_ctx)
    } else {
        let message = "save load: snapshot lost its cell/exterior context between queue and drain";
        log::error!("{message}");
        notify_player(world, message);
        return;
    };
    let Some(ReloadOutcome {
        location_label,
        count_label,
    }) = outcome
    else {
        return;
    };

    // The cell/session replacement invalidates every transient SDK entity
    // handle even when the restored population happens to reuse the same raw
    // EntityId. Rebind extension rows only through stable FormRef identity;
    // unavailable packages/forms stay retained verbatim for a later load/save.
    if let Err(error) = crate::extensions::restore_extension_state(world, &snapshot) {
        let message =
            format!("save load: extension-state restore failed after preflight: {error:#}");
        log::error!("{message}");
        notify_player(world, message);
        return;
    }

    // Re-assert saved resources over anything the reload rebuilt
    // (`CurrentCellContext`, `PlayerPose`), so inventory instance ids
    // resolve, then overlay the form-id-keyed mutable deltas. The
    // spawn-time consumers were served by the pre-reload restore above
    // (#3789); this second pass is the one that has always been here.
    if let Err(e) = byroredux_save::restore_resources(world, &registry, &snapshot) {
        let message = format!("save load: resource restore failed: {e}");
        log::error!("{message}");
        notify_player(world, message);
        return;
    }
    let remap = byroredux_save::build_form_id_remap(world, &registry, &snapshot);
    match byroredux_save::apply_deltas(world, &registry, &snapshot, &remap, MUTABLE_DELTA_COLUMNS) {
        Ok(applied) => {
            let dead = crate::combat::reconcile_dead_actor_runtime_state(world);
            // #3488 — the overlay is additive-only, so a saved *absence*
            // (unarmed) cannot clear a live `EquippedWeapon` off the
            // process-lifetime player body, which survives the cell reload
            // untouched. Re-derive it from the `EquipmentSlots` + `Inventory`
            // rows just overlaid, using the same reconciler the runtime
            // equip/unequip path uses. Sibling of the `Dead` reconciler
            // above, per the contract in `byroredux_save::apply_deltas`.
            let player = world
                .try_resource::<crate::systems::PlayerEntity>()
                .and_then(|entity| entity.0);
            if let Some(player) = player {
                crate::inventory::reconcile_player_equipped_weapon(world, player);
            }
            log::info!(
                "save load: {location_label} reloaded ({count_label}); applied {} saved deltas \
                 across {} form-id-matched entities; reconciled {} dead actors",
                applied,
                remap.len(),
                dead
            );
        }
        Err(e) => {
            // Typed decoding already succeeded in the preflight above, so
            // this arm is an unexpected apply failure. Do not continue into
            // validation or pose restore on a potentially partial overlay.
            let dead = crate::combat::reconcile_dead_actor_runtime_state(world);
            let message = format!(
                "save load: delta apply failed after preflight: {e}; reconciled {dead} dead actors before aborting"
            );
            log::error!("{message}");
            notify_player(world, message);
            return;
        }
    }

    // #1844 / SAVE-01 — mirror the save path's `validate_world` +
    // `validate_form_ids` pairing (see `SaveCommand::execute` above) as a
    // post-load diagnostic. A save written before a given validation
    // rule existed, or a hand-edited-but-CRC-valid file, would otherwise
    // overlay a referentially broken world with no warning. Diagnostic
    // only — a load can't cleanly fall back to the previous cell.
    let mut issues = validate_world(world);
    issues.extend(validate_form_ids(world));
    issues.extend(validate_cinematic_entity_refs(world));
    log_validation_warnings(&format!("save load: {location_label}"), &issues);

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
            if pose.character_mode {
                "character"
            } else {
                "flycam"
            },
        );
    }
    if let Err(error) = crate::extensions::queue_session_event(
        world,
        byroredux_sdk::event::SessionEvent {
            phase: byroredux_sdk::event::SessionPhase::LoadComplete,
            slot: Some(loaded_slot),
        },
    ) {
        log::warn!(
            "load slot {loaded_slot} completed, but its extension lifecycle event was not queued: {error}"
        );
    }
}

// Tests live in sibling files by topic (#2407 / TD1-004) — the repo's
// existing convention (`cell_loader/*_tests.rs`,
// `scene_buffer/*_tests.rs`). Contents moved verbatim.
#[cfg(test)]
mod command_queue_tests;
#[cfg(test)]
mod live_reload_tests;
#[cfg(test)]
mod registry_completeness_tests;
#[cfg(test)]
mod round_trip_tests;
#[cfg(test)]
mod serde_default_guard_tests;
#[cfg(test)]
mod validation_gate_tests;
