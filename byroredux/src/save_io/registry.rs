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
    // #6759 — the player's per-REPU fame/infamy (u32 FormID + two u16), written by the
    // `AddReputation` / `SetReputation` ObScript commands and the SDK; nothing re-derives it.
    "FactionReputation",
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
    // VATS-LIMB-009 - a u16 bitset of body locations and an optional u8 killing-blow location: no
    // session-local identity. Replacing: a saved absence clears a live record.
    "DismemberRecord",
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

/// Build the curated game-state save registry.
///
/// Only types that carry *player-visible game state* are registered —
/// derived data (`GlobalTransform`, `WorldBound`), GPU handles
/// (`MeshHandle`, `TextureHandle`, `SkinnedMesh`), and transient event
/// markers are reconstructed on load, never serialised.
///
/// #6705 — a scripting subsystem's saved columns are registered by its own
/// `register_saved` (`SaveRegistry::schema_fingerprint` hashes components and
/// resources in registration order, so each call sits where its rows sat). What
/// is spelled here are the rows whose type cannot do that: core types
/// (`byroredux-core` sits below `byroredux-save`), the physics character
/// controller, and the binary crate's own state.
pub fn build_save_registry() -> SaveRegistry {
    use byroredux_core::animation::{AnimationPlayer, AnimationStack};
    use byroredux_core::ecs::components::{
        ActorValues, ActorVitals, Children, Dead, EquipmentSlots, EquippedWeapon, EscortState,
        Escorted, FollowState, GuardState, Inventory, LightFlicker, LightSource, Material, Name,
        Parent, PatrolState, Seated, Transform, TravelState, Traveled, WanderState,
    };
    use byroredux_core::ecs::resources::ItemInstancePool;

    use crate::cell_loader::{CurrentCellContext, CurrentExteriorContext};
    use crate::components::GameTimeRes;

    let mut r = SaveRegistry::new();
    // Core types: `byroredux-core` sits below `byroredux-save`, so they cannot register themselves (#6705).
    r.register_component::<Transform>("Transform");
    r.register_component::<Name>("Name");
    r.register_component::<Parent>("Parent");
    r.register_component::<Children>("Children");
    r.register_component::<Inventory>("Inventory");
    r.register_component::<EquipmentSlots>("EquipmentSlots");
    r.register_component::<LightSource>("LightSource");
    r.register_component::<LightFlicker>("LightFlicker");
    r.register_component::<AnimationPlayer>("AnimationPlayer");
    r.register_component::<AnimationStack>("AnimationStack");
    byroredux_scripting::timer::register_saved(&mut r);
    byroredux_scripting::vm_state::register_saved(&mut r);
    // Core types: `byroredux-core` sits below `byroredux-save`, so they cannot register themselves (#6705).
    // #1834 — layered actor values (SPECIAL / skills / resistances /
    // resources / derived). Stamped at NPC spawn from class auto-calc and
    // mutated live by `setav`/`modav`; pre-fix a save/load silently
    // reverted every edited/permanent/temporary/damage layer to the
    // re-derived spawn base. Also a MUTABLE_DELTA_COLUMN (delta-safe).
    r.register_component::<ActorValues>("ActorValues");
    r.register_replacing_component::<byroredux_core::character::Perks>("Perks");
    // M47.2 — kill experience accumulates `xp` and rolls levels, so
    // #2947's re-derived exemption (and its `validate_progression_state`
    // guard) ended.
    r.register_replacing_component::<byroredux_core::character::LevelUpPoints>("LevelUpPoints");
    r.register_replacing_component::<byroredux_core::character::CharacterLevel>("CharacterLevel");
    // #6759 — player-scoped like `CharacterLevel` above (the player outlives cell reloads), so a
    // reputation earned by a quest script survives a save/load instead of resetting to Neutral.
    r.register_replacing_component::<byroredux_core::character::FactionReputation>("FactionReputation");
    // VATS-LIMB-009 - which body locations are recorded gone and where the killing blow landed; written by the
    // combat funnel and read by `IsLimbGone` / `GetKillingBlowLimb`, nothing re-derives it. Replacing, like
    // `Dead`: a saved absence really means "no record", so a live actor's record is cleared by a load that
    // predates it.
    r.register_replacing_component::<byroredux_core::character::DismemberRecord>("DismemberRecord");
    r.register_replacing_component::<byroredux_core::ecs::components::TimedRestorations>("TimedRestorations");
    byroredux_scripting::magic::register_saved(&mut r);
    // Core types: `byroredux-core` sits below `byroredux-save`, so they cannot register themselves (#6705).
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
    r.register_component::<ActorVitals>("ActorVitals");
    r.register_component::<EquippedWeapon>("EquippedWeapon");
    // #5027 — replacing: the process-lifetime player outlives the cell
    // reload, so an additive overlay left a pre-load death standing on a
    // save where the player was alive. Saved absence on a FormID-matched
    // actor is authoritative; resident NPCs respawn alive regardless.
    r.register_replacing_component::<Dead>("Dead");
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
    r.register_component::<WanderState>("WanderState");
    r.register_component::<TravelState>("TravelState");
    r.register_component::<Traveled>("Traveled");
    r.register_component::<FollowState>("FollowState");
    r.register_component::<EscortState>("EscortState");
    r.register_component::<Escorted>("Escorted");
    r.register_component::<GuardState>("GuardState");
    r.register_component::<PatrolState>("PatrolState");
    r.register_component::<Seated>("Seated");
    byroredux_scripting::cinematic::register_saved(&mut r);
    byroredux_scripting::player_control::register_saved(&mut r);
    byroredux_scripting::bleedout::register_saved(&mut r);
    byroredux_scripting::script_packages::register_saved(&mut r);
    // Physics type: `byroredux-physics` has no `byroredux-save` dependency (#6705).
    r.register_component::<byroredux_physics::CharacterController>("CharacterController");
    // Core types: `byroredux-core` sits below `byroredux-save`, so they cannot register themselves (#6705).
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
    r.register_component::<Material>("Material");
    byroredux_scripting::papyrus_demo::register_saved(&mut r);
    // Core types: `byroredux-core` sits below `byroredux-save`, so they cannot register themselves (#6705).
    r.register_form_id_component("FormIdComponent");
    r.register_resource::<ItemInstancePool>("ItemInstancePool");
    // Binary-crate types register here (#6705).
    r.register_resource::<crate::cell_loader::reference_state::PersistentReferenceStates>("PersistentReferenceStates");
    // M45.1 — the cell identity + plugin set the save was taken in,
    // so `load` knows which cell to reload before applying deltas.
    r.register_resource::<CurrentCellContext>("CurrentCellContext");
    // EX-09/17 item 4 — the exterior-mode counterpart to
    // `CurrentCellContext`: worldspace/grid/radius identity for a save
    // taken mid-exterior-streaming, so `load` can rebuild the same
    // `WorldStreamingState` instead of rejecting exterior saves outright.
    r.register_resource::<CurrentExteriorContext>("CurrentExteriorContext");
    // M45.1 refinement — where the player was standing + looking, so
    // `load` restores the pose instead of the cell's default spawn.
    r.register_resource::<PlayerPose>("PlayerPose");
    // M34 day/night completion — the clock is mutable gameplay state:
    // weather advances it every frame and `time.*` controls can change
    // hour/rate/day. Restoring after the cell reload lets the next
    // weather tick re-derive sky, fog, sun, and directional lighting.
    r.register_resource::<GameTimeRes>("GameTimeRes");
    // Core types: `byroredux-core` sits below `byroredux-save`, so they cannot register themselves (#6705).
    r.register_resource::<byroredux_core::ecs::resources::HardcoreMode>("HardcoreMode");
    byroredux_scripting::quest_stages::register_saved(&mut r);
    byroredux_scripting::story_manager::register_saved(&mut r);
    byroredux_scripting::quest_stages::register_saved_quest_objective_state(&mut r);
    byroredux_scripting::quest_variables::register_saved(&mut r);
    byroredux_scripting::faction_changes::register_saved(&mut r);
    byroredux_scripting::location::register_saved(&mut r);
    // M47.2 — Created Object "At" references: identity, base and
    // placement, so a quest's created reference respawns in its cell.
    byroredux_scripting::created_references::register_saved(&mut r);
    byroredux_scripting::alias_overrides::register_saved(&mut r);
    byroredux_scripting::fragment::register_state_saved(&mut r);
    byroredux_scripting::map_markers::register_saved(&mut r);
    byroredux_scripting::location::register_saved_location_clear_state(&mut r);
    byroredux_scripting::relationships::register_saved(&mut r);
    byroredux_scripting::quest_variables::register_saved_quest_script_objects(&mut r);
    byroredux_scripting::fragment::register_updates_saved(&mut r);
    // M47.2 — Game.IncrementStat misc stats and Game.AddAchievement awards.
    byroredux_scripting::misc_stats::register_saved(&mut r);
    // M47.2 — ObjectReference.SetLinkedRef links.
    byroredux_scripting::linked_refs::register_saved(&mut r);
    byroredux_scripting::obscript_quests::register_saved(&mut r);
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
    // Who knows of which crime (witnesses, the guards they call): GetCrime / GetCrimeKnown.
    byroredux_scripting::crime_knowledge::register_saved(&mut r);
    // M47.3 — the New Vegas challenges' progress, unlocks and recurring flags.
    byroredux_scripting::challenges::register_saved(&mut r);
    // M47.3 — the magic effects running on an actor (script effects, timed value changes).
    byroredux_scripting::magic_effects::register_saved(&mut r);
    // M47.2 — Actor.SetBribed / SetIntimidated flags.
    byroredux_scripting::speechcraft::register_saved(&mut r);
    // M47.2 — Actor.ChangeAnimArchetype assignments.
    byroredux_scripting::animation_graph::register_saved(&mut r);
    // M47.2 — Starfield SetScanned / AddToMapScanned flags.
    byroredux_scripting::scanning::register_saved(&mut r);
    // M47.2 — FormList.AddForm / RemoveAddedForm / Revert.
    byroredux_scripting::form_lists::register_saved(&mut r);
    // M47.2 — ObjectReference.SetLocRefType assignments.
    byroredux_scripting::loc_ref_types::register_saved(&mut r);
    byroredux_scripting::fragment::register_alias_scripts_saved(&mut r);
    byroredux_scripting::fragment::register_anim_events_saved(&mut r);
    // M47.2 — destructible references' damage.
    byroredux_scripting::destruction::register_saved(&mut r);
    byroredux_scripting::fragment::register_hits_saved(&mut r);
    // M47.2 — Location.SetKeywordData data.
    byroredux_scripting::location_keyword_data::register_saved(&mut r);
    byroredux_scripting::reference_flags::register_saved(&mut r);
    byroredux_scripting::input_layers::register_saved(&mut r);
    // M47.2 — Actor.SetPlayerTeammate / SetDoingFavor flags.
    byroredux_scripting::followers::register_saved(&mut r);
    byroredux_scripting::fragment::register_distance_saved(&mut r);
    byroredux_scripting::alias_collections::register_saved(&mut r);
    byroredux_scripting::fragment::register_events_saved(&mut r);
    // M47.2 — Game.AdvanceSkill's banked usage XP per skill.
    byroredux_scripting::skills::register_saved(&mut r);
    // M43 — Game.TeachWord / UnlockWord: the words of power the player has.
    byroredux_scripting::words_of_power::register_saved(&mut r);
    byroredux_scripting::keywords::register_saved(&mut r);
    byroredux_scripting::bleedout::register_saved_actor_base_protection_overrides(&mut r);
    byroredux_scripting::dialogue::register_saved(&mut r);
    byroredux_scripting::globals::register_saved(&mut r);
    byroredux_scripting::scene::register_quest_alias_saved(&mut r);
    byroredux_scripting::player_control::register_saved_player_control_state(&mut r);
    byroredux_scripting::fragment::register_state_fragment_execution_queue_saved(&mut r);
    byroredux_scripting::papyrus_provider::register_ir_saved(&mut r);
    byroredux_scripting::fragment::register_state_reference_enable_state_saved(&mut r);
    byroredux_scripting::cinematic::register_saved_cinematic_presentation_state(&mut r);
    byroredux_scripting::combat::register_saved(&mut r);
    r
}
