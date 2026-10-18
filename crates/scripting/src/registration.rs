//! #5806 — the scripting crate's world setup, split out of `lib.rs`
//! (which stays the public re-export facade). Every M47.2 subsystem
//! registers its default resource here; the `world.init_resource::<T>()`
//! calls replace the three-line insert-if-absent blocks this function
//! used to spell 37 times.

use crate::*;
use byroredux_core::ecs::World;

/// Register all scripting component storages in the world.
///
/// Call during setup so that `query_mut()` works for event markers
/// before any entity has triggered an event.
pub fn register(world: &mut World) {
    world.insert_resource(CompatibilityRegistry::default());
    // M47.3 — the lines scripts made actors say (`Say` / `SayTo`; not saved).
    world.init_resource::<crate::obscript_speech::PendingScriptedLines>();
    // M47.2 — the scripted faction-membership ledger (saved; a restore
    // replaces this default).
    world.init_resource::<ScriptedFactionChanges>();
    // M47.2 — script relationship ranks (saved).
    world.init_resource::<ScriptedRelationships>();
    // M47.2 — cleared locations (saved).
    world.init_resource::<LocationClearState>();
    // M47.2 — scripted AddToMap (saved).
    world.init_resource::<MapMarkers>();
    // M47.2 — scripted IncrementStat misc stats (saved).
    world.init_resource::<MiscStats>();
    // M47.2 — alias scripts: the authored ones (derived) and their owner ids
    // and started quests (saved).
    world.init_resource::<QuestAliasScripts>();
    world.init_resource::<AliasScriptOwners>();
    // M47.2 — what the reference scripts last saw of the actors (transient).
    world.init_resource::<ActorWatch>();
    // M47.2 — unload events awaiting their frame (transient).
    world.init_resource::<PendingUnloadDeliveries>();
    // M47.2 — Fallout 4 / Starfield input enable layers (saved).
    world.init_resource::<InputLayers>();
    // M47.2 — flags scripts set on references (saved).
    world.init_resource::<ReferenceFlags>();
    // M47.2 — Sound.Play requests awaiting the host (transient).
    world.init_resource::<PendingParentActivations>();
    world.init_resource::<PendingDetectionEvents>();
    world.init_resource::<PendingProjectileLaunches>();
    world.init_resource::<PendingSoundPlays>();
    // M47.2 — ProcessTrapHit requests awaiting the Update flush (transient).
    world.init_resource::<PendingTrapHits>();
    // M47.2 — reference owners, faction crime gold and crime factions: each
    // subsystem installs its own resources (#5802).
    ownership::install(world);
    crime_gold::install(world);
    crime_faction::install(world);
    crate::obscript_state::install(world);
    crate::magic_effects::install(world);
    crate::magic_shaders::install(world);
    world.register::<crate::obscript_anim::HeadLook>();
    world.init_resource::<crate::destruction::PendingStageChanges>();
    // M47.2 — form lists scripts read (derived).
    world.init_resource::<FormLists>();
    // M47.2 — the weather scripts can ask about (derived).
    world.init_resource::<CurrentWeather>();
    world.init_resource::<WeatherClassifications>();
    // M47.2 — keyword data scripts attached to locations (saved).
    world.init_resource::<LocationKeywordData>();
    // M47.2 — destructible objects' damage (saved).
    world.init_resource::<destruction::DestructionStates>();
    // M47.2 — Papyrus animation event registrations (saved).
    world.init_resource::<AnimationEventRegistrations>();
    // M47.2 — Papyrus hit event registrations (saved).
    world.init_resource::<HitRegistrations>();
    // M47.2 — scripted SetLinkedRef links (saved).
    world.init_resource::<LinkedRefOverrides>();
    // M47.2 — SetBribed / SetIntimidated flags and ChangeAnimArchetype
    // assignments (saved).
    world.init_resource::<SpeechcraftOutcomes>();
    world.init_resource::<AnimArchetypeOverrides>();
    // M47.2 — Starfield SetScanned / AddToMapScanned flags (saved).
    world.init_resource::<ScannedReferences>();
    // M47.2 — forms scripts added to form lists (saved).
    world.init_resource::<FormListAdditions>();
    // M47.2 — scripted SetLocRefType assignments (saved).
    world.init_resource::<LocRefTypeOverrides>();
    // M47.2 — Game.AdvanceSkill: banked usage XP (saved) and the advances
    // scripts queued (transient).
    world.init_resource::<SkillProgress>();
    world.init_resource::<PendingSkillAdvances>();
    // M47.2 — script-filled reference-collection alias members (saved).
    world.init_resource::<AliasCollections>();
    // M47.2 — scripted SetPlayerTeammate / SetDoingFavor flags (saved).
    world.init_resource::<FollowerState>();
    // M47.2 — scripted AddAchievement awards (saved).
    world.init_resource::<Achievements>();
    // M47.2 — scripted ForceRefTo / Clear alias overrides (saved).
    world.init_resource::<AliasOverrides>();
    // M47.2 — created-object references (saved, like the ledger above).
    world.init_resource::<CreatedReferences>();
    // M47.2 — quest location-alias bindings (saved, like the ledger above).
    world.init_resource::<LocationAliasBindings>();
    // M47.2 — the Papyrus interpreter's per-frame allowance (#5763); transient,
    // renewed by `event_cleanup_system` at the end of every frame.
    world.init_resource::<VmFrameBudget>();
    world.register::<ActivateEvent>();
    world.register::<NativeActivation>();
    world.register::<HitEvent>();
    world.register::<SplashEvent>();
    world.register::<RippleEvent>();
    world.register::<TimerExpired>();
    world.register::<AnimationTextKeyEvents>();
    world.register::<ScriptTimer>();
    // #4415 — actor spell lists (stamped at spawn, mutated by AddSpell /
    // RemoveSpell effects through `query_mut`, so pre-registered).
    world.register::<SpellList>();
    // #4415 — the racial SPLO set `AddRaceSpells`/`RemoveRaceSpells` use.
    world.register::<RaceSpells>();
    // Canonical event markers. OnInitEvent + OnCellLoadEvent +
    // OnTriggerEnterEvent + EquipmentEventBatch join the existing
    // ActivateEvent / HitEvent / TimerExpired in the script-event
    // catalog. Emit sites land per-phase:
    //   * OnCellLoadEvent — emitted by the cell loader's
    //     `attach_script_for_refr` (Phase 5).
    //   * OnTriggerEnterEvent — emitted by `trigger_detection_system`
    //     on player entry (M47.2).
    //   * EquipmentEventBatch — emitted by player and scripted equip paths.
    // All four are one-frame transients drained by `event_cleanup_system`.
    world.register::<OnCellLoadEvent>();
    world.register::<OnInitEvent>();
    world.register::<OnTriggerEnterEvent>();
    world.register::<crate::OnTriggerLeaveEvent>();
    world.register::<OnDeathEvent>();
    world.register::<EquipmentEventBatch>();
    // P3 — item add/remove channel (future `OnItemAdded`/`OnItemRemoved`).
    // Emitted by loot transfers (whole/selective) and world pickups; drained
    // by `event_cleanup_system` like the equipment batch above. No reader
    // yet (#4713).
    world.register::<ItemEventBatch>();
    // M47.2 — trigger-volume storage. The cell loader attaches a
    // `TriggerVolume` to each invisible trigger REFR; `trigger_detection_system`
    // emits `OnTriggerEnterEvent` on player entry, which the quest-advance
    // dispatch consumes (the `default*Trigger` family).
    trigger::register(world);
    combat::register(world);
    obscript_runtime::register(world);
    papyrus_provider::register(world);
    recurring_update::register(world);
    quest_stages::register(world);
    // #5366 — Story Manager dispatch (transient StoryEvent markers).
    story_manager::register(world);
    // M47.2 (b2) — quest-stage fragment dispatch. Registers the
    // QuestStageAdvanced component storage (via quest_advance below it's
    // also registered, idempotent) and the QuestStageFragments /
    // QuestObjectiveState resources the dispatcher reads.
    fragment::register(world);
    // Skyrim+ SCEN orchestration: immutable definitions live in a resource,
    // while one ScenePlayer component per record owns mutable playback state.
    scene::register(world);
    // DIAL/INFO selection and persistent subtitle state consume the scene
    // runtime's dialogue starts and return canonical action completions.
    dialogue::register(world);
    // Skyrim+ PACK template resolution and scene-owned package execution.
    package::register(world);
    equipment::register(world);
    player_control::register(world);
    bleedout::register(world);
    keywords::register(world);
    menus::register(world);
    messages::register(world);
    cinematic::register(world);
    vm_state::register(world);
    // M47.0 Phase 1 — register the R5 prototype storages so
    // `papyrus_demo` scripts can attach their state components when
    // their owning REFR spawns. Without this call, `query_mut::<…>`
    // returns None for every script-state component and the demo
    // systems no-op. The cell-loader-driven attach lands in Phase 3;
    // this Phase 1 step is the minimum plumbing that makes the demo
    // surface live at runtime. See docs/engine/m47-0-design.md.
    papyrus_demo::register(world);
    log::info!("Scripting subsystem initialized (ECS events + timers + papyrus_demo)");
}
