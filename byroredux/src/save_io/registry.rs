//! Save registry + save-slot discovery (#5585, split from
//! `save_io.rs`).
//!
//! Owns the curated [`SaveRegistry`] the binary installs (which
//! component/resource columns a save carries), the mutable-delta
//! allowlist the live load overlays, and where save slots live on
//! disk ([`SaveState`] / [`discover_save_dir`]).

use super::*;

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
pub(crate) const MUTABLE_DELTA_COLUMNS: &[&str] = &[
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
    // Ranked perk FormIDs (u32) and ranks (u8) are session-stable. Its
    // replacing registration also clears a saved absence on the live player.
    "Perks",
    // M47.2 — level and XP toward the next (u16 / u32); kill experience
    // accumulates it, so it is progression, not a re-derived spawn stamp.
    "CharacterLevel",
    // M47.2 — unspent level-up skill points (u32 + an f32 carry) and perks.
    "LevelUpPoints",
    // Stable source/AV FormIDs and scalar rate/remaining simulation seconds.
    "TimedRestorations",
    // #4415 — spell FormIDs (u32) only.
    "SpellList",
    // Combat state is session-stable: the weapon points into the saved
    // Inventory by u32 index and Dead is a zero-field lifecycle marker.
    // Dead's replacing registration clears a saved absence on the live
    // player (#5027).
    "EquippedWeapon",
    "Dead",
    // #2014 / SAVE-D1-NEW-01 — delta-safe subset of the seven M42
    // AI-procedure runtime-state components: WanderState/TravelState/
    // GuardState/PatrolState are plain Vec3/enum/u32 fields, and
    // Traveled/Escorted are empty terminal markers. FollowState/
    // EscortState/Seated are deliberately NOT here — they carry
    // `EntityId` fields (`target_entity`/`furniture`), the same
    // session-local-reference hazard `#1696` already excluded
    // `AnimationPlayer.root_entity` for. `Seated` carries a SECOND,
    // independent session-local-handle hazard too (#3333/#3791):
    // `animation_restore.clip_handle` is an `AnimationClipRegistry` index
    // — precisely the handle class the registry's own allowlist entry
    // rejects it for ("numeric handles are session-local"). Both hazards
    // are checked pre-save by `validate_animation`
    // (`crates/save/src/validate.rs`), not just the `EntityId` one this
    // comment used to name alone. Those three still ride the full
    // register_component round-trip above, just not the live overlay.
    "WanderState",
    "TravelState",
    "Traveled",
    "GuardState",
    "PatrolState",
    "Escorted",
    // #2292 / SAVE-D1-09 — `ActorControlState { restrained, unconscious,
    // ghost }` (#5017 added the second bool, M47.2 `SetGhost` the third). No session-local identity. Replacing registration (#5052): a
    // saved absence clears a live restraint on the process-lifetime player.
    "ActorControlState",
    // M47.2 — `Bleedout { remaining_secs: Option<f32> }` (#5858). No session-local identity.
    // Replacing: a saved absence gets a live bleeding actor back up.
    "Bleedout",
    // M47.3 — `ScriptPackage { package: u32, constrained: bool, running_secs:
    // Option<f32> }`, the package an ObScript `AddScriptPackage` gave an actor.
    // No session-local identity. Replacing: a saved absence drops a live one.
    "ScriptPackage",
    // M47.3 — `WeaponDrawn` / `IgnoreCrime`: unit markers, presence is the flag (ObScript `SetAlert` /
    // `IgnoreCrime`). `ActiveMagicEffects`: the effects running on an actor — FormIDs, seconds, a script's
    // locals by slot; no session-local identity. Replacing: a saved absence clears a live one.
    "WeaponDrawn",
    "IgnoreCrime",
    "ActiveMagicEffects",
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

/// #5686 — the live-load overlay hook for `RigidBodyData`
/// (`register_component_preserving`): `part_surface_materials` is
/// `serde(skip)` — derived from the NIF collision tree at import, the
/// #3470 class — so applying a saved row wholesale used to wipe the
/// freshly re-imported slice and collapse every compound collider part
/// onto the body-level material. Keep the live slice when the
/// deserialized row carries none (always, today — forward-compatible if
/// the field ever joins the snapshot). `restore_world` never runs this
/// hook; there is no live row to preserve.
fn keep_reimported_part_surface_materials(
    incoming: &mut byroredux_core::ecs::components::RigidBodyData,
    live: &byroredux_core::ecs::components::RigidBodyData,
) {
    if incoming.part_surface_materials.is_empty() && !live.part_surface_materials.is_empty() {
        incoming.part_surface_materials = live.part_surface_materials.clone();
    }
    // `havok_layer` is derived from the NIF the same way (`serde(skip)`): a
    // deserialized row reads 0, so the re-imported layer is kept.
    if incoming.havok_layer == 0 {
        incoming.havok_layer = live.havok_layer;
    }
}

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
        QuestAliasInjectionState, ReferenceEnableState, ReferenceLockState, ReferenceScriptState,
        ScriptTimer, ScriptVariables,
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
        .register_replacing_component::<byroredux_core::character::Perks>("Perks")
        // M47.2 — kill experience accumulates `xp` and rolls levels, so
        // #2947's re-derived exemption (and its `validate_progression_state`
        // guard) ended.
        .register_replacing_component::<byroredux_core::character::LevelUpPoints>(
            "LevelUpPoints",
        )
        .register_replacing_component::<byroredux_core::character::CharacterLevel>(
            "CharacterLevel",
        )
        .register_replacing_component::<byroredux_core::ecs::components::TimedRestorations>("TimedRestorations")
        // #4415 — the actor's spells. Stamped at spawn from SPLO and mutated
        // by AddSpell / RemoveSpell; the ActorValues column above already
        // carries the permanent changes those spells made, so the two must
        // round-trip together.
        .register_component::<byroredux_scripting::SpellList>("SpellList")
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
        // `CharacterLevel` below, just not listed in
        // `REDERIVED_NOT_SAVED` because it's plain-registered (not the
        // save-omitted case that allowlist tracks).
        .register_component::<ActorVitals>("ActorVitals")
        .register_component::<EquippedWeapon>("EquippedWeapon")
        // #5027 — replacing: the process-lifetime player outlives the cell
        // reload, so an additive overlay left a pre-load death standing on a
        // save where the player was alive. Saved absence on a FormID-matched
        // actor is authoritative; resident NPCs respawn alive regardless.
        .register_replacing_component::<Dead>("Dead")
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
        // silently freed it on reload. #5052 — replacing: the first
        // `SetRestrained` inserts it lazily on the process-lifetime player,
        // so loading a save with no row must clear a live restraint rather
        // than leave the player unable to move.
        .register_replacing_component::<ActorControlState>("ActorControlState")
        // M47.2 — an essential actor saved mid-bleedout reloads still down.
        .register_replacing_component::<byroredux_scripting::Bleedout>("Bleedout")
        // M47.3 — a package a quest script gave an actor reloads still its package.
        .register_replacing_component::<byroredux_scripting::script_packages::ScriptPackage>("ScriptPackage")
        .register_component::<byroredux_physics::CharacterController>("CharacterController")
        // #2379 / SAVE-D1-14 — `motion_type` (+ mass/friction/restitution/
        // damping) is mutated at runtime by Papyrus `.SetMotionType()`
        // (`scripted_motion_type_system`), not just static bhkRigidBody
        // import data. Flat enum + f32s, delta-safe. Pre-fix a scripted
        // motion-type change (e.g. making a prop dynamic for a scripted
        // sequence) silently reverted to the ESM-derived default on reload.
        // #5686 — preserving, not plain: `part_surface_materials` is
        // `serde(skip)` (re-derived from the NIF collision tree at
        // import), so the live-load overlay must keep the freshly
        // re-imported slice instead of overwriting it with the row's
        // empty default.
        .register_component_preserving::<RigidBodyData>(
            "RigidBodyData",
            keep_reimported_part_surface_materials,
        )
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
        .register_resource::<crate::cell_loader::reference_state::PersistentReferenceStates>(
            "PersistentReferenceStates",
        )
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
        .register_resource::<byroredux_core::ecs::resources::HardcoreMode>("HardcoreMode")
        // #1862 / SAVE-07 — quest stage/objective progress is live gameplay
        // state (Papyrus `SetStage`/`GetStage`/`GetStageDone` and
        // `SetObjectiveDisplayed`/`SetObjectiveCompleted`/`SetObjectiveFailed`),
        // mutated every frame by real recognizer-emitted scripts
        // (quest_advance, dlc2_ttr4a, mg07_door). Pre-fix it silently reverted
        // to default on every save/load.
        .register_resource::<QuestStageState>("QuestStageState")
        // #5366 Phase 3 — Story Manager node-policy state (do-all fired
        // marks, RNAM reset timestamps), so a quickload cannot resurrect
        // a radiant the pre-save world already fired.
        .register_resource::<byroredux_scripting::StoryManagerNodeState>(
            "StoryManagerNodeState",
        )
        // #5394 — an SM-started quest restores as running and its starting
        // event is never raised again, so its `FromEvent` aliases can only
        // re-fill from this record (reference slots persist as FormIds).
        .register_resource::<byroredux_scripting::StoryEventAliasFill>(
            "StoryEventAliasFill",
        )
        .register_resource::<QuestObjectiveState>("QuestObjectiveState")
        // M47.2 — quest-script property writes (`kmyQuest.Flag = 1`) that
        // `GetVMQuestVariable` dialogue / package / scene conditions read;
        // only the writes serialize (the VMAD defaults are re-derived).
        .register_resource::<byroredux_scripting::QuestScriptVariables>(
            "QuestScriptVariables",
        )
        // M47.2 — scripted AddToFaction / RemoveFromFaction / SetFactionRank,
        // replayed over the record-derived `FactionRanks` at spawn and load.
        .register_resource::<byroredux_scripting::ScriptedFactionChanges>(
            "ScriptedFactionChanges",
        )
        // M47.2 — quest location-alias bindings. A "Find Matching Location"
        // fill picks at random, so a reload must keep the pick rather than
        // re-roll it.
        .register_resource::<byroredux_scripting::LocationAliasBindings>(
            "LocationAliasBindings",
        )
        // M47.2 — Created Object "At" references: identity, base and
        // placement, so a quest's created reference respawns in its cell.
        .register_resource::<byroredux_scripting::CreatedReferences>("CreatedReferences")
        // M47.2 — scripted ForceRefTo / Clear alias overrides, by reference
        // FormID; dropped when their quest stops.
        .register_resource::<byroredux_scripting::AliasOverrides>("AliasOverrides")
        // M47.2 — script BlockActivation, by reference FormID (outlives cells).
        .register_resource::<byroredux_scripting::ReferenceActivationBlocks>(
            "ReferenceActivationBlocks",
        )
        // M47.2 — script AddToMap (discovered / fast-travel markers).
        .register_resource::<byroredux_scripting::MapMarkers>("MapMarkers")
        // M47.2 — cleared locations (dead bosses, script SetCleared).
        .register_resource::<byroredux_scripting::LocationClearState>("LocationClearState")
        // M47.2 — script SetRelationshipRank changes.
        .register_resource::<byroredux_scripting::ScriptedRelationships>("ScriptedRelationships")
        // M47.2 — the Papyrus VM's object script variables.
        .register_resource::<byroredux_scripting::QuestScriptObjects>("QuestScriptObjects")
        // M47.2 — Papyrus RegisterForUpdate / RegisterForSingleUpdate (and
        // the GameTime pair) registrations and the clocks they count.
        .register_resource::<byroredux_scripting::ScriptUpdates>("ScriptUpdates")
        // M47.2 — Game.IncrementStat misc stats.
        .register_resource::<byroredux_scripting::MiscStats>("MiscStats")
        // M47.2 — Game.AddAchievement awards.
        .register_resource::<byroredux_scripting::Achievements>("Achievements")
        // M47.2 — ObjectReference.SetLinkedRef links.
        .register_resource::<byroredux_scripting::LinkedRefOverrides>("LinkedRefOverrides")
        // M47.3 — the ObScript quest scripts' local variables.
        .register_resource::<byroredux_scripting::obscript_quests::ObScriptQuestLocals>("ObScriptQuestLocals");
    // M47.2 — SetActorOwner / SetFactionOwner, faction crime gold and
    // Actor.SetCrimeFaction: each subsystem registers its own column (#5802),
    // here, at its place in the row order the schema fingerprint hashes.
    byroredux_scripting::ownership::register_saved(&mut r);
    byroredux_scripting::crime_gold::register_saved(&mut r);
    byroredux_scripting::crime_faction::register_saved(&mut r);
    // M47.3 — Pip-Boy notes, kill records, a forced weather, and the weapon-out / ignore-crime flags.
    byroredux_scripting::obscript_state::register_saved(&mut r);
    // M47.3 — fame, bounty, faction crime flags, known topics, dispositions and the `Set...` values.
    byroredux_scripting::obscript_ledger::register_saved(&mut r);
    // M47.3 — the New Vegas challenges' progress, unlocks and recurring flags.
    byroredux_scripting::challenges::register_saved(&mut r);
    // M47.3 — the magic effects running on an actor (script effects, timed value changes).
    byroredux_scripting::magic_effects::register_saved(&mut r);
    r
        // M47.2 — Actor.SetBribed / SetIntimidated flags.
        .register_resource::<byroredux_scripting::SpeechcraftOutcomes>("SpeechcraftOutcomes")
        // M47.2 — Actor.ChangeAnimArchetype assignments.
        .register_resource::<byroredux_scripting::AnimArchetypeOverrides>("AnimArchetypeOverrides")
        // M47.2 — Starfield SetScanned / AddToMapScanned flags.
        .register_resource::<byroredux_scripting::ScannedReferences>("ScannedReferences")
        // M47.2 — FormList.AddForm / RemoveAddedForm / Revert.
        .register_resource::<byroredux_scripting::FormListAdditions>("FormListAdditions")
        // M47.2 — ObjectReference.SetLocRefType assignments.
        .register_resource::<byroredux_scripting::LocRefTypeOverrides>("LocRefTypeOverrides")
        // M47.2 — alias script owner ids and the quests whose alias scripts started.
        .register_resource::<byroredux_scripting::AliasScriptOwners>("AliasScriptOwners")
        // M47.2 — Papyrus hit event registrations.
        .register_resource::<byroredux_scripting::AnimationEventRegistrations>("AnimationEventRegistrations")
        .register_resource::<byroredux_scripting::destruction::DestructionStates>("DestructionStates")
        .register_resource::<byroredux_scripting::HitRegistrations>("HitRegistrations")
        // M47.2 — Location.SetKeywordData data.
        .register_resource::<byroredux_scripting::LocationKeywordData>("LocationKeywordData")
        // M47.2 — SetNoFavorAllowed / SetActorCause flags.
        .register_resource::<byroredux_scripting::ReferenceFlags>("ReferenceFlags")
        // M47.2 — InputEnableLayer handles and the controls they disable.
        .register_resource::<byroredux_scripting::InputLayers>("InputLayers")
        // M47.2 — Actor.SetPlayerTeammate / SetDoingFavor flags.
        .register_resource::<byroredux_scripting::FollowerState>("FollowerState")
        // M47.2 — Papyrus distance-event registrations.
        .register_resource::<byroredux_scripting::DistanceWatches>("DistanceWatches")
        // M47.2 — script-filled RefCollectionAlias members.
        .register_resource::<byroredux_scripting::AliasCollections>("AliasCollections")
        // M47.2 — Papyrus custom / remote event registrations.
        .register_resource::<byroredux_scripting::EventRegistrations>("EventRegistrations")
        // M47.2 — Game.AdvanceSkill's banked usage XP per skill.
        .register_resource::<byroredux_scripting::SkillProgress>("SkillProgress")
        // M47.2 — script AddKeyword / RemoveKeyword changes.
        .register_resource::<byroredux_scripting::keywords::ScriptedKeywords>("ScriptedKeywords")
        // M47.2 — script ActorBase.SetEssential / SetProtected.
        .register_resource::<byroredux_scripting::ActorBaseProtectionOverrides>(
            "ActorBaseProtectionOverrides",
        )
        // #5367 Phase L — spoken Say-Once INFO bookkeeping: a said line
        // stays disqualified after an F5/F9 cycle, exactly the
        // quest-state contract beside it.
        .register_resource::<byroredux_scripting::DialogueSpokenInfoForms>(
            "DialogueSpokenInfoForms",
        )
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
        // Utility.Wait.
        //
        // #4139 — this comment used to end "All nested identities are stable
        // manifest strings; no EntityId or process-local handles cross the
        // save boundary." That is false, and it is why this column was
        // registered without a rebind step: `locals` is a
        // `BTreeMap<String, ScriptValue>`, and `ScriptValue::Entity` carries
        // an `EntityRef` — a handle the SDK's own doc calls "never a raw ECS
        // slot ... scoped to a world_generation so a stale one is
        // rejectable", i.e. process-local by definition. The route/tail and
        // principal halves of the claim are correct; the locals half was not.
        //
        // Continuations carrying such a local are dropped on load rather
        // than resumed — see `drop_entity_bound_continuations` and the call
        // site in `execute_pending_save_loads`.
        .register_resource::<PapyrusProviderContinuationQueue>("PapyrusProviderContinuationQueue")
        .register_resource::<ReferenceEnableState>("ReferenceEnableState")
        // #4136 — the lock ledger's sibling. `Effect::SetLocked` /
        // `SetLockLevel` (#3159) made `Locked` runtime-mutable with nothing
        // persisting the change: absent from this registry, and re-stamped
        // from the plugin's authored XLOC on every cell load. Keyed by
        // local FormID and consulted by `cell_loader::spawn`, so it
        // survives both a save/load and an in-session cell revisit — the
        // latter being why this is a resource and not a saved `Locked`
        // column, since cell unload despawns the entity that would carry
        // one. Plain `u8` / `Option<u32>` payloads, no session-local
        // handles.
        .register_resource::<ReferenceLockState>("ReferenceLockState")
        // #4334 — the once-only-script ledger, third sibling of the same
        // shape: `onlyOnce` triggers park their script in a terminal
        // `GotoState` without disabling the reference, and the parked
        // state must survive both a save/load and an in-session cell
        // revisit so the attach path does not re-arm them. Plain local
        // FormID keys, no session-local handles.
        .register_resource::<ReferenceScriptState>("ReferenceScriptState")
        // #2380 / SAVE-D1-15 — MQ101 cinematic presentation state
        // (sitting rotation, animation-event registrations, active IMAD
        // applications). No `EntityId`/`FixedString` anywhere.
        // `image_space_modifier_catalog` is static ESM data that rides
        // along redundantly (see the type's own doc comment).
        .register_resource::<CinematicPresentationState>("CinematicPresentationState")
        // #4414 — scripted faction reactions (`Faction.SetEnemy`), which the
        // ambient hostility system now reads. Keyed by portable `FormRef`
        // (plugin identity + local object id), never a load-order slot, so
        // a reload under a changed load order still names the same factions.
        .register_resource::<byroredux_scripting::FactionRelations>("FactionRelations");
    r
}
