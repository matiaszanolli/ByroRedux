//! #5806 — the scripting crate's world setup, split out of `lib.rs`
//! (which stays the public re-export facade). Every M47.2 subsystem
//! registers its default resource through its own `install` (#6706; each
//! calls `world.init_resource::<T>()`, which replaced the three-line
//! insert-if-absent blocks this function used to spell 37 times).

use crate::*;
use byroredux_core::ecs::World;

/// Register all scripting component storages in the world.
///
/// Call during setup so that `query_mut()` works for event markers
/// before any entity has triggered an event.
pub fn register(world: &mut World) {
    compatibility::install(world);
    obscript_speech::install(world);
    faction_changes::install(world);
    relationships::install(world);
    location::install(world);
    map_markers::install(world);
    // M47.2 — scripted IncrementStat misc stats and AddAchievement awards
    // (saved): `misc_stats` installs both (#5802).
    misc_stats::install(world);
    globals::install(world);
    input_layers::install(world);
    reference_flags::install(world);
    activate_parents::install(world);
    detection_events::install(world);
    projectile_requests::install(world);
    sound_requests::install(world);
    trap_hit_requests::install(world);
    // M47.2 — reference owners, faction crime gold and crime factions: each
    // subsystem installs its own resources (#5802).
    ownership::install(world);
    crime_gold::install(world);
    crime_faction::install(world);
    crate::obscript_state::install(world);
    crate::obscript_ledger::install(world);
    crate::crime_knowledge::install(world);
    jail::install(world);
    crate::challenges::install(world);
    crate::obscript_more::install(world);
    crate::magic_effects::install(world);
    crate::magic_shaders::install(world);
    world.register::<crate::obscript_anim::HeadLook>();
    // #4128 — PlayGroup flag 0's held sequence request (not saved).
    world.register::<crate::obscript_anim::PendingGroupSwap>();
    // M47.2 — form lists scripts read (derived) and the forms scripts added to
    // them (saved): `form_lists` installs both (#5802).
    form_lists::install(world);
    weather_state::install(world);
    // M47.2 — keyword data scripts attached to locations (saved, #5802).
    location_keyword_data::install(world);
    // M47.2 — destructible objects' damage and stage changes, SetBribed /
    // SetIntimidated, SetLocRefType and Game.AdvanceSkill: each subsystem
    // installs its own resources (#5802).
    destruction::install(world);
    speechcraft::install(world);
    loc_ref_types::install(world);
    skills::install(world);
    words_of_power::install(world);
    tempering::install(world);
    perk_entry_points::install_weapon_subject(world);
    // M47.2 — scripted SetLinkedRef links, ChangeAnimArchetype assignments and
    // Starfield SetScanned / AddToMapScanned flags: each subsystem installs its
    // own resources (#5802). The form-list additions come with `form_lists`.
    linked_refs::install(world);
    animation_graph::install(world);
    scanning::install(world);
    alias_collections::install(world);
    // M47.2 — scripted SetPlayerTeammate / SetDoingFavor flags (saved, #5802).
    followers::install(world);
    alias_overrides::install(world);
    // M47.2 — created-object references (saved, like the ledger above; #5802).
    created_references::install(world);
    world.register::<ActivateEvent>();
    world.register::<NativeActivation>();
    world.register::<HitEvent>();
    world.register::<HitShots>();
    world.register::<HitParts>();
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
    // PERK-BODY — what the perk Ability/Quest reconcile last saw of each perk holder.
    world.register::<crate::perk_entry_points::PerkBodyState>();
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

#[cfg(test)]
mod tests {
    use super::*;

    /// #6706 — `register` moved its direct resource setup into per-subsystem
    /// `install` functions. The count is the resource set `register` produced
    /// before the move (138, +2 for XCT: `ActorRelocations`, `PendingRelocations`, +1 for `PendingNonArmorEquips`,
    /// +1 for XCT-LOC-004's `PendingActorMoves`, +1 for BIK-PRES-006's `MoviePlaying`); the list is
    /// the types it used to initialise by hand, so a dropped `install` call fails by name rather
    /// than by count alone.
    #[test]
    fn register_installs_the_same_resource_set() {
        let mut world = World::new();
        register(&mut world);
        assert_eq!(world.resource_count(), 143, "the resource set `register` installs changed");

        macro_rules! present {
            ($($ty:path),+ $(,)?) => {$(
                assert!(
                    world.try_resource::<$ty>().is_some(),
                    concat!(stringify!($ty), " is no longer installed by `register`")
                );
            )+};
        }
        present!(
        crate::obscript_speech::PendingScriptedLines,
        crate::faction_changes::ScriptedFactionChanges,
        crate::relationships::ScriptedRelationships,
        crate::location::LocationClearState,
        crate::map_markers::MapMarkers,
        crate::globals::GameCalendar,
        crate::fragment::QuestAliasScripts,
        crate::fragment::AliasScriptOwners,
        crate::fragment::ActorWatch,
        crate::fragment::PendingUnloadDeliveries,
        crate::input_layers::InputLayers,
        crate::reference_flags::ReferenceFlags,
        crate::activate_parents::PendingParentActivations,
        crate::detection_events::PendingDetectionEvents,
        crate::projectile_requests::PendingProjectileLaunches,
        crate::sound_requests::PendingSoundPlays,
        crate::trap_hit_requests::PendingTrapHits,
        crate::jail::PendingEvidenceDeposits,
        crate::weather_state::CurrentWeather,
        crate::weather_state::WeatherClassifications,
        crate::fragment::AnimationEventRegistrations,
        crate::fragment::HitRegistrations,
        crate::alias_collections::AliasCollections,
        crate::alias_overrides::AliasOverrides,
        crate::location::LocationAliasBindings,
        crate::fragment::VmFrameBudget,
        crate::compatibility::CompatibilityRegistry,
        );
    }
}
