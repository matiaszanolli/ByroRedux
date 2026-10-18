//! `Stage::Update` registrations (#3855, split from `boot.rs`).
//!
//! The largest stage, and the one carrying the nested dispatch shims that
//! adapt per-game runtimes to the scheduler's `fn(&World, f32)` shape.

use byroredux_core::ecs::{Access, ActiveCamera, Scheduler, Stage, Transform, World};
use byroredux_core::string::StringPool;

use crate::interaction::{
    ActionState, InteractionCandidateScratch, InteractionState, InteractionTrace,
};
use crate::systems::{
    animate_lights_system, loading_model_turntable_system, make_animation_system, spin_system,
};

/// `Stage::Update` registrations (#3739 split of `build_scheduler`).
pub(super) fn register_update_systems(scheduler: &mut Scheduler) {
    scheduler.add_exclusive_with_access(
        Stage::Update,
        crate::systems::restoration::restoration_system,
        Access::new()
            .reads::<byroredux_core::ecs::components::ActorVitals>()
            .reads::<byroredux_core::ecs::components::Dead>()
            .writes::<byroredux_core::ecs::components::ActorValues>()
            .writes::<byroredux_core::ecs::components::TimedRestorations>(),
    );
    // M47.2 — essential actors get back up when their bleedout runs out.
    scheduler.add_exclusive_with_access(
        Stage::Update,
        byroredux_scripting::bleedout_system,
        Access::new()
            .reads_resource::<byroredux_scripting::BleedoutRules>()
            .writes::<byroredux_scripting::Bleedout>()
            .reads::<byroredux_core::ecs::components::Dead>()
            .reads::<byroredux_core::ecs::components::ActorVitals>()
            .writes::<byroredux_core::ecs::components::ActorValues>(),
    );
    // M47.2 — the player's carried weight against Carry Weight; the
    // movement effect reads it next frame.
    scheduler.add_exclusive_with_access(
        Stage::Update,
        crate::encumbrance::encumbrance_system,
        Access::new()
            .reads_resource::<crate::encumbrance::EncumbranceRules>()
            .reads_resource::<crate::systems::PlayerEntity>()
            .reads_resource::<crate::inventory::InventoryCatalog>()
            .reads::<byroredux_core::ecs::components::Inventory>()
            .writes::<byroredux_core::ecs::components::ActorValues>()
            .writes_resource::<crate::encumbrance::PlayerEncumbrance>(),
    );
    // M47.0 Phase 1 — R5 papyrus_demo dispatchers. These are
    // event-driven (early-return when no ActivateEvent /
    // OnUpdateEvent / RecurringUpdate is present), so they
    // contribute zero work to frames without scripted activity.
    // Registered as exclusive in Update so they run serially
    // after the parallel batch — keeps M27's zero-conflict
    // report intact without forcing each demo to declare its
    // (component-heavy) access surface. Declared access lands
    // when the demos move to Stage::Script in a follow-up; for
    // now they ride the same "trivial gameplay system =
    // exclusive" lane as spin_system / footstep_system /
    // particle_system. See docs/engine/m47-0-design.md.
    //
    // Signature note: the 6 World-only demos predate engine
    // wiring (R5 was a prototype against unit tests that called
    // them directly). The blanket `impl<F: FnMut(&World, f32)>
    // System for F` requires the `dt` arg, so we adapt via inline
    // closures that drop the unused parameter. Renaming the demo
    // signatures themselves would break ~30 unit-test call sites
    // in papyrus_demo/tests.rs that intentionally call without
    // dt; closures are cheaper.
    // F7 (2026-05-27) — wrap each `&World`-only papyrus-demo
    // system as a local `fn` item so it gets a unique
    // `std::any::type_name`. The previous closures all shared the
    // auto-generated name `byroredux::App::new::{{closure}}`,
    // tripping the scheduler's `has_system(name)` duplicate-name
    // probe and emitting 5+ "duplicate exclusive system" warnings
    // per launch (cosmetic — the systems still all got added).
    // Local fn items each have their own unique type and unique
    // type-name path, so the duplicate check passes naturally.
    fn rumble_on_activate_dispatch(world: &World, _dt: f32) {
        byroredux_scripting::papyrus_demo::rumble_on_activate_system(world)
    }
    fn trigger_detection_dispatch(world: &World, _dt: f32) {
        byroredux_scripting::trigger_detection_system(world)
    }
    fn quest_advance_dispatch(world: &World, _dt: f32) {
        byroredux_scripting::papyrus_demo::quest_advance::quest_advance_system(world)
    }
    // #5366 — Story Manager producers + dispatch. The wrapper shapes
    // exist so the registrations below read as a pair: the CLOC
    // producer raises the marker, the dispatcher drains and walks the
    // tree in the same frame.
    fn story_change_location_dispatch(world: &World, _dt: f32) {
        crate::systems::story_events::story_change_location_system(world)
    }
    fn story_manager_dispatch(world: &World, _dt: f32) {
        // #5366 Phase 3 — sync the Story Manager's clock from the
        // canonical game time (advanced by weather_system in Early)
        // before dispatch reads it for RNAM reset windows.
        if let Some(time) = world.try_resource::<crate::components::GameTimeRes>() {
            if let Some(mut clock) =
                world.try_resource_mut::<byroredux_scripting::StoryClock>()
            {
                clock.hours = time.day as f64 * 24.0 + f64::from(time.hour);
            }
        }
        byroredux_scripting::story_manager_dispatch_system(world)
    }
    // SCR-D6-NEW-02 (#1768) — the runtime scripting systems that were
    // registered (component/resource) but never scheduled. Both ride
    // the same exclusive-in-Update lane as the demo dispatchers above:
    // exclusive registration holds the world serially, so the fragment
    // system's resource-lock sequence never composes with a parallel
    // neighbour. `quest_fragment_dispatch_system` is `&World`-only, so
    // it needs the dt-dropping wrapper; `recurring_update_tick_system`
    // already has the `(&World, f32)` shape and is added directly.
    fn quest_fragment_dispatch(world: &World, _dt: f32) {
        byroredux_scripting::quest_fragment_dispatch_system(world)
    }
    // M47.2 — Papyrus update registrations count the host's game clock.
    fn script_update_dispatch(world: &World, dt: f32) {
        let game_hours = world
            .try_resource::<crate::components::GameTimeRes>()
            .map(|time| time.absolute_hours());
        // M47.3 — the `GameHour` global is the game clock (CS *GetCurrentTime*).
        if let Some((day, hour)) = world.try_resource::<crate::components::GameTimeRes>().map(|time| (time.day, time.hour)) {
            byroredux_scripting::globals::sync_game_clock_globals(world, day, hour);
        }
        byroredux_scripting::script_update_system(world, dt, game_hours)
    }
    fn check_alias_scripts_dispatch(world: &World, _dt: f32) {
        crate::asset_provider::check_running_quest_alias_scripts(world)
    }
    fn dlc2_ttr4a_on_init_dispatch(world: &World, _dt: f32) {
        byroredux_scripting::papyrus_demo::dlc2_ttr4a::dlc2_ttr4a_on_init_system(world)
    }
    fn dlc2_ttr4a_on_update_dispatch(world: &World, _dt: f32) {
        byroredux_scripting::papyrus_demo::dlc2_ttr4a::dlc2_ttr4a_on_update_system(world)
    }
    fn mg07_on_load_dispatch(world: &World, _dt: f32) {
        byroredux_scripting::papyrus_demo::mg07_door::mg07_on_load_system(world)
    }
    fn mg07_on_activate_dispatch(world: &World, _dt: f32) {
        byroredux_scripting::papyrus_demo::mg07_door::mg07_on_activate_system(world)
    }
    // Canonical player interaction runs before every OnActivate consumer so
    // a fresh E-key edge is visible to scripts in the same frame.
    //
    // #3473 — the three P2 gameplay exclusives below were added after #2391
    // and inherited plain `add_exclusive`, so the newest and least-reviewed
    // subsystem reported blank `sys.accesses` rows while carrying the
    // deepest hold stack in the schedule (`combat_input_system`'s
    // `EquippedWeapon` -> CHARAL chain, since flattened in `combat.rs`).
    // Same rationale as `pool_regen_tick_system` below: the analyzer never
    // pairs exclusives, so these declarations surface no conflict row —
    // their job is to put the disputed types on the report instead of a
    // blank row, and to give `BYRO_LOCK_ORDER_CHECK`'s recorded edges a
    // declaration to be compared against if either system is ever promoted
    // to a parallel lane.
    //
    // Scope: each declaration covers the system body and the helpers it
    // calls directly (including `queue_door_transition`'s cell-index /
    // plugin-set / transition-slot triple). It deliberately stops at
    // `reconcile_dead_actor`'s ragdoll activation, whose own physics
    // surface is declared by `ragdoll_writeback_system`.
    scheduler.add_exclusive_with_access(
        Stage::Update,
        crate::interaction::interaction_system,
        Access::new()
            .writes_resource::<crate::notifications::PlayerNotifications>()
            .reads_resource::<byroredux_core::form_id::FormIdPool>()
            .reads::<byroredux_core::ecs::components::FormIdComponent>()
            .writes_resource::<byroredux_scripting::ReferenceLockState>()
            .writes::<byroredux_core::ecs::components::Locked>()
            .reads_resource::<crate::systems::PlayerEntity>()
            .reads::<byroredux_core::ecs::components::Dead>()
            .reads::<byroredux_physics::ActorColliderOwner>()
            .reads::<byroredux_physics::Ragdoll>()
            .reads_resource::<crate::inventory::InventoryCatalog>()
            .reads::<byroredux_scripting::SceneAliasCandidate>()
            // #5035 — the Npc Talk arm's bulk passes: the candidate filter
            // queries `ActorValues` directly, and
            // `running_quest_bound_entities` reads the quest-alias tables
            // (`installed_alias_ids_by_quest` + the running-quest filter).
            .reads::<byroredux_core::ecs::components::ActorValues>()
            .reads_resource::<byroredux_scripting::SceneQuestAliasRegistry>()
            .reads_resource::<byroredux_scripting::SceneActorBindings>()
            .reads_resource::<byroredux_scripting::quest_stages::QuestStageState>()
            // #5043 / #5017 — the Talk arm skips actors in combat or
            // unconscious.
            .reads::<byroredux_scripting::AiCombatState>()
            .reads::<byroredux_scripting::ActorControlState>()
            .reads::<byroredux_core::ecs::components::Inventory>()
            .reads_resource::<ActionState>()
            .reads_resource::<ActiveCamera>()
            .reads_resource::<byroredux_physics::PhysicsWorld>()
            // #4574 — collider_belongs_to_target reads the source form
            // (reached through target_has_line_of_sight); the same
            // helper's FormIdComponent and ActorColliderOwner are declared
            // above.
            .reads::<byroredux_core::ecs::components::PhysicsSourceForm>()
            .reads_resource::<byroredux_scripting::papyrus_demo::PapyrusPlayerEntity>()
            .reads_resource::<crate::cell_loader::LoadedCellIndex>()
            .reads_resource::<crate::cell_loader::LoadedPluginSet>()
            .writes_resource::<InteractionState>()
            .writes_resource::<InteractionTrace>()
            .writes_resource::<InteractionCandidateScratch>()
            .writes_resource::<crate::cell_loader::PendingCellTransitionSlot>()
            .reads::<Transform>()
            .reads::<byroredux_core::ecs::GlobalTransform>()
            .reads::<byroredux_core::ecs::WorldBound>()
            .reads::<byroredux_physics::RapierHandles>()
            .reads::<crate::components::DoorTeleport>()
            .reads::<byroredux_core::ecs::components::Locked>()
            .reads::<byroredux_scripting::TwoStateActivator>()
            .reads::<byroredux_scripting::papyrus_demo::RumbleOnActivate>()
            .reads::<byroredux_scripting::papyrus_demo::quest_advance::QuestAdvanceOnActivate>()
            .reads::<byroredux_scripting::papyrus_demo::mg07_door::MG07LabyrinthianDoor>()
            // #4697 — pickup candidates skip taken placements; #4698 —
            // disabled placements are filtered against the enable ledger.
            .reads::<crate::inventory::PickedUp>()
            .reads_resource::<byroredux_scripting::ReferenceEnableState>()
            .writes::<byroredux_scripting::ActivateEvent>(),
    );
    // #2654 — quest fragments queue their `<Ref>.Activate()` targets rather
    // than inserting `ActivateEvent` directly, because
    // `quest_fragment_dispatch` runs *after* most of the consumers below (it
    // has to: it consumes the `QuestStageAdvanced` markers
    // `quest_advance_dispatch` emits) and `event_cleanup_system` drains the
    // marker at Stage::Late the same frame. Flush the queue here, directly
    // after the canonical player-interaction producer, so a fragment
    // activation reaches every consumer exactly once on the following frame.
    // #4712 — that includes `container_loot_system` immediately below: a
    // flush registered after it turned a scripted `ContainerRef.Activate()`
    // or loose-item `Activate()` into a silent no-op.
    scheduler.add_exclusive(
        Stage::Update,
        byroredux_scripting::fragment_activation_flush_system,
    );
    // #5868 — a scripted / relayed activation of a load door by the player
    // opens it (the E-press path queues its own transition directly).
    scheduler.add_exclusive(Stage::Update, crate::interaction::door_activation_system);
    scheduler.add_exclusive_with_access(
        Stage::Update,
        crate::inventory::container_loot_system,
        Access::new()
            .writes_resource::<crate::notifications::PlayerNotifications>()
            .reads::<byroredux_core::ecs::components::Dead>()
            .writes::<byroredux_core::ecs::components::EquipmentSlots>()
            .writes::<byroredux_core::ecs::components::EquippedWeapon>()
            .writes::<byroredux_scripting::EquipmentEventBatch>()
            .reads_resource::<crate::systems::PlayerEntity>()
            .reads_resource::<crate::inventory::InventoryCatalog>()
            .reads::<byroredux_scripting::ActivateEvent>()
            .reads::<byroredux_scripting::SceneAliasCandidate>()
            .reads::<byroredux_core::ecs::components::Locked>()
            // #4574 — pickup_loot stamps PickedUp on the target and its
            // subtree meshes; transfer_is_theft reads ownership.
            .writes::<crate::inventory::PickedUp>()
            .reads::<byroredux_core::ecs::components::Owned>()
            // #5866 — the theft rule reads a script's ownership change first.
            .reads_resource::<byroredux_scripting::OwnerOverrides>()
            .reads::<byroredux_core::ecs::components::FactionRanks>()
            // #4706 — the placed stack size pickup_loot grants.
            .reads::<crate::inventory::PlacedItemCount>()
            .writes::<byroredux_core::ecs::components::Inventory>()
            // #4996 — pickup_loot's helpers. `mesh_entities_under` walks the
            // subtree; `mark_picked_up` resolves the ref's FormId and records
            // it; `remove_collision_bodies` (#4818) matches the ref's Havok
            // bodies by source form and removes them from Rapier. Exclusive,
            // so no race today — but the row must be complete for the
            // analyzer if this ever joins a parallel batch.
            .reads::<byroredux_core::ecs::Children>()
            .reads::<byroredux_core::ecs::MeshHandle>()
            .reads::<byroredux_core::ecs::components::FormIdComponent>()
            .reads_resource::<byroredux_core::form_id::FormIdPool>()
            .writes_resource::<crate::cell_loader::reference_state::PersistentReferenceStates>()
            .reads::<byroredux_core::ecs::components::PhysicsSourceForm>()
            .reads::<byroredux_physics::RapierHandles>()
            .writes_resource::<byroredux_physics::PhysicsWorld>(),
    );
    // Combat follows the same producer-before-consumer event contract as
    // activation: physical Attack emits HitEvent, then health/death resolves
    // before script consumers and Late-stage transient cleanup.
    scheduler.add_exclusive_with_access(
        Stage::Update,
        crate::combat::combat_input_system,
        Access::new()
            .reads_resource::<ActionState>()
            .reads_resource::<crate::systems::PlayerEntity>()
            .reads_resource::<crate::systems::PlayerMode>()
            .reads_resource::<ActiveCamera>()
            .reads_resource::<byroredux_physics::PhysicsWorld>()
            .reads_resource::<byroredux_core::character::MeleeDamageConfig>()
            .reads_resource::<byroredux_core::character::CharacterRuleset>()
            .writes_resource::<crate::combat::CombatState>()
            .reads::<Transform>()
            .reads::<byroredux_core::ecs::GlobalTransform>()
            .reads::<byroredux_physics::RapierHandles>()
            .reads::<byroredux_physics::ActorColliderOwner>()
            .reads::<byroredux_core::ecs::components::EquippedWeapon>()
            .reads::<byroredux_core::ecs::components::ActorVitals>()
            .reads::<byroredux_core::ecs::components::ActorValues>()
            .reads::<byroredux_core::character::CharacterLevel>()
            .reads::<byroredux_core::ecs::components::Dead>()
            // #4574 — the cooldown arm's own query_mut/query on MeleeState
            // (#3709) and attack_damage's CreatureAttack read.
            .reads::<crate::combat::MeleeState>()
            .writes::<crate::combat::MeleeState>()
            .reads::<byroredux_core::ecs::components::CreatureAttack>()
            .writes::<byroredux_scripting::HitEvent>(),
    );
    // #4414 — ambient faction hostility: an NPC that sees an actor its
    // faction relations and aggression say it attacks arms `AiCombatState`,
    // the same state `Effect::StartCombat` arms. Scheduled just before the
    // combat AI so a combat started this frame is fought this frame.
    scheduler.add_exclusive_with_access(
        Stage::Update,
        crate::systems::make_faction_hostility_system(),
        Access::new()
            .reads_resource::<crate::systems::DetectionConfig>()
            .reads_resource::<crate::components::CellLightingRes>()
            .reads_resource::<crate::systems::PlayerEntity>()
            .reads_resource::<byroredux_scripting::LoadOrderIdentity>()
            .reads_resource::<byroredux_scripting::FactionRelations>()
            .reads_resource::<crate::cell_loader::load_order::GlobalFormIdResolver>()
            .reads_resource::<byroredux_physics::PhysicsWorld>()
            .reads::<crate::systems::CombatDisposition>()
            .reads::<byroredux_core::ecs::components::Dead>()
            .reads::<byroredux_core::ecs::components::GlobalTransform>()
            .reads::<byroredux_core::ecs::components::FactionRanks>()
            .reads::<byroredux_physics::RapierHandles>()
            .reads::<byroredux_scripting::AiCombatState>()
            .writes::<byroredux_scripting::AiCombatState>()
            // #5017 — unconscious perceivers start no combat.
            .reads::<byroredux_scripting::ActorControlState>()
            // #4816 — the ambient-combat marker it stamps, ages and drops.
            .reads::<crate::systems::AmbientEngagement>()
            .writes::<crate::systems::AmbientEngagement>(),
    );
    // MQ101's dragon-attack/keep-escape combat gate (stages 270+, ROADMAP.md
    // "MQ101 end-to-end playability"): `Effect::StartCombat` arms
    // `AiCombatState`; this system chases and strikes exactly like the
    // player's own `combat_input_system` above, so it shares the same
    // producer-before-consumer HitEvent contract — scheduled after the
    // player's own attack input and before `combat_damage_system` resolves
    // the frame's HitEvents.
    scheduler.add_exclusive_with_access(
        Stage::Update,
        crate::systems::make_npc_combat_ai_system(),
        Access::new()
            .reads_resource::<byroredux_physics::PhysicsWorld>()
            .reads_resource::<byroredux_core::character::MeleeDamageConfig>()
            .reads_resource::<byroredux_core::character::CharacterRuleset>()
            .reads::<Transform>()
            .writes::<Transform>()
            .reads::<byroredux_core::ecs::components::Dead>()
            .reads::<byroredux_core::ecs::components::EquippedWeapon>()
            .reads::<byroredux_core::ecs::components::CreatureAttack>()
            .reads::<byroredux_core::ecs::components::ActorValues>()
            .reads::<byroredux_core::character::CharacterLevel>()
            .reads::<byroredux_scripting::AiCombatState>()
            .writes::<byroredux_scripting::AiCombatState>()
            // #5017 — an unconscious attacker's combat ends.
            .reads::<byroredux_scripting::ActorControlState>()
            // #4574 — the chase arm reads the authored stride (M42.11).
            .reads::<crate::components::WalkSpeed>()
            // #4703 — each live attacker's ambient package is suspended
            // through `clear_ambient_behavior`: the runtime winner reset,
            // the seat released and its animation park undone, plus every
            // ambient Behavior/State pair that function removes.
            .reads::<crate::components::AmbientPackageRuntime>()
            .writes::<crate::components::AmbientPackageRuntime>()
            .reads::<byroredux_core::ecs::components::Seated>()
            .writes::<byroredux_core::ecs::components::Seated>()
            .writes_resource::<crate::components::SeatReservations>()
            .writes::<byroredux_core::animation::AnimationPlayer>()
            // #4821 — the pairs themselves, removed through the generic
            // `remove_component::<T>`, were claimed above but never declared.
            .writes::<byroredux_core::ecs::components::SandboxBehavior>()
            .writes::<byroredux_core::ecs::components::WanderBehavior>()
            .writes::<byroredux_core::ecs::components::WanderState>()
            .writes::<byroredux_core::ecs::components::TravelBehavior>()
            .writes::<byroredux_core::ecs::components::TravelState>()
            .writes::<byroredux_core::ecs::components::Traveled>()
            .writes::<byroredux_core::ecs::components::FollowBehavior>()
            .writes::<byroredux_core::ecs::components::FollowState>()
            .writes::<byroredux_core::ecs::components::EscortBehavior>()
            .writes::<byroredux_core::ecs::components::EscortState>()
            .writes::<byroredux_core::ecs::components::Escorted>()
            .writes::<byroredux_core::ecs::components::GuardBehavior>()
            .writes::<byroredux_core::ecs::components::GuardState>()
            .writes::<byroredux_core::ecs::components::PatrolBehavior>()
            .writes::<byroredux_core::ecs::components::PatrolState>()
            // M42 Eat/Sleep + the Dialogue force-greet bridge — same
            // `clear_ambient_behavior` teardown family as the pairs
            // above.
            .writes::<byroredux_core::ecs::components::EatBehavior>()
            .writes::<byroredux_core::ecs::components::SleepBehavior>()
            .writes::<byroredux_core::ecs::components::EatSleepState>()
            .writes::<crate::systems::forcegreet::ForceGreetDirective>()
            .writes::<crate::components::WalkStuckTimer>()
            .writes::<crate::components::NavPath>()
            .writes::<byroredux_scripting::HitEvent>(),
    );
    scheduler.add_exclusive(Stage::Update, rumble_on_activate_dispatch);
    scheduler.add_exclusive(
        Stage::Update,
        byroredux_scripting::papyrus_demo::rumble_tick_system,
    );
    // M47.2 — trigger detection runs BEFORE quest_advance so an
    // OnTriggerEnterEvent emitted this frame is consumed the same
    // frame (before end-of-frame cleanup drains it).
    scheduler.add_exclusive(Stage::Update, trigger_detection_dispatch);
    // Offscreen scene actors can reach cataloged trigger REFRs whose cells
    // are not resident. Emit their logical OnTriggerEnter handoff here so
    // quest_advance consumes it in the same frame.
    scheduler.add_exclusive(
        Stage::Update,
        crate::systems::make_scene_trigger_actor_approach_system(),
    );
    scheduler.add_exclusive(Stage::Update, quest_advance_dispatch);
    // ESM data is installed before the player/event sink exists. Bootstrap
    // Start Game Enabled quests here so Begin On Quest Start scenes observe
    // the same-frame transition.
    scheduler.add_exclusive(Stage::Update, byroredux_scripting::quest_startup_system);
    // #5366 — Story Manager: raise CLOC on session-location change, then
    // dispatch every raised story event. Runs right after quest_startup
    // and before the alias refresh so an SM-started quest is
    // indistinguishable from a Start Game Enabled one to everything
    // downstream (aliases, scenes, fragments) — and the KILL marker the
    // combat damage system raises is drained here too, one frame later
    // at the latest.
    scheduler.add_exclusive(Stage::Update, story_change_location_dispatch);
    scheduler.add_exclusive(Stage::Update, story_manager_dispatch);
    // Quest aliases are a general QUST facility, not a SCEN implementation
    // detail. Refresh after startup/cell candidate changes even when the load
    // order contains no scene records; the dirty fast path is allocation-free.
    scheduler.add_exclusive(
        Stage::Update,
        byroredux_scripting::quest_alias_refresh_system,
    );
    scheduler.add_exclusive(
        Stage::Update,
        byroredux_scripting::quest_alias_readiness_stage_system,
    );
    // SCEN playback must observe the original quest-start batch before the
    // fragment dispatcher can replace the shared sink with chained SetStage
    // advances. Phase conditions affected by those fragments are retried on
    // the following tick.
    scheduler.add_exclusive(Stage::Update, byroredux_scripting::scene_playback_system);
    // Execute SCEN VMAD begin/end/phase fragments immediately after playback
    // emits them. Any SetStage effect enters the canonical quest journal for
    // the quest-fragment dispatcher later in this same update.
    scheduler.add_exclusive(
        Stage::Update,
        byroredux_scripting::scene_fragment_dispatch_system,
    );
    // M42.9 / #2652 — ambient packages observe EvaluatePackageRequest before
    // the SCEN package system drains the transient marker. Time-driven checks
    // are bounded to one pass per in-game minute inside the system.
    scheduler.add_exclusive(Stage::Update, crate::npc_spawn::ambient_ai_package_system);
    // M47.2 — the package transitions it reports run their PF_ fragments.
    scheduler.add_exclusive(Stage::Update, byroredux_scripting::package_fragment_system);
    // M47.2 — the perk entry runs the host reports run their PRKF_ fragments.
    scheduler.add_exclusive(Stage::Update, byroredux_scripting::perk_fragment_system);
    // M47.2 — the terminal menu picks the host reports run their TERM_ fragments.
    scheduler.add_exclusive(Stage::Update, byroredux_scripting::terminal_fragment_system);
    // #5367 Phase F — force-greet directives (Dialogue-procedure
    // packages) walk their NPCs to the player and open the conversation
    // without activation. After the ambient package system so a fresh
    // install greets on the next tick; before the Late dialogue
    // selection so the opened surface serials once.
    //
    // #5430 — the approach step is AI motion, so it honours the
    // `BYRO_NO_AI_LOCOMOTION` kill switch; the in-radius open does not.
    // M47.3 — `StartConversation player <topic>` requests become directives first.
    scheduler.add_exclusive(Stage::Update, crate::systems::forcegreet::scripted_conversation_system);
    scheduler.add_exclusive(
        Stage::Update,
        crate::systems::forcegreet::make_forcegreet_system(super::ai_locomotion_enabled()),
    );
    // M47.3 — legacy ObScript quest scripts (Oblivion/FO3/FNV): run each
    // running quest's GameMode block on the vanilla 5 s cadence. Exclusive
    // beside `ambient_ai_package_system`: it drives `QuestStageState`
    // through the same stage events the Papyrus fragment path emits, and
    // the timer maps must stay race-free.
    scheduler.add_exclusive(
        Stage::Update,
        byroredux_scripting::obscript_quests::obscript_quest_tick_system,
    );
    // PACK actions resolve their Skyrim PKCU template/data inputs, move actors
    // toward authored invisible-marker coordinates for Travel-family leaves,
    // and queue Done completions for scene playback's next tick.
    scheduler.add_exclusive(Stage::Update, byroredux_scripting::scene_package_system);
    // Package Activate leaves emit the same canonical OnActivate marker as
    // player interaction — but they QUEUE it through
    // `PendingFragmentActivations` rather than inserting it here (#3936),
    // because `scene_package_system` above runs after
    // `rumble_on_activate_dispatch` and `quest_advance_dispatch`. The
    // early `fragment_activation_flush_system` delivers it to every
    // consumer next frame. This registration consumes the flushed
    // marker so GetVMScriptVariable phase gates observe the updated
    // two-state activator, before end-of-frame event cleanup.
    scheduler.add_exclusive(
        Stage::Update,
        byroredux_scripting::two_state_activator_system,
    );
    // Dialogue consumes the ActionStarted batch emitted immediately above,
    // exposes authored INFO subtitle/presentation state for the rest of the
    // frame, and queues completions for scene playback's next tick.
    scheduler.add_exclusive(Stage::Update, byroredux_scripting::scene_dialogue_system);
    // M47.3 — legacy ObScript object scripts (the `SCRI` scripts on placed
    // references): `GameMode` every frame, `OnLoad` and `OnDeath` on their
    // markers. Before `reference_script_event_system` below, which drains
    // `OnDeathEvent` at its head; after the package, activation and dialogue
    // systems whose state the commands read.
    // M47.3 — a cell that reset as the player entered it owes its scripted references an `OnReset`.
    scheduler.add_exclusive(Stage::Update, byroredux_scripting::obscript_state::mark_cell_reset);
    scheduler.add_exclusive(
        Stage::Update,
        byroredux_scripting::obscript_objects::obscript_object_system,
    );
    // M47.3 — the lines `Say` / `SayTo` made actors say: the voice player picks up this
    // frame's, then the timers tick and finished lines run their `SayToDone` blocks.
    scheduler.add_exclusive(Stage::Update, crate::systems::dialogue_voice::scripted_line_voice_system);
    scheduler.add_exclusive(Stage::Update, byroredux_scripting::obscript_speech::obscript_speech_system);
    // M47.2 — reference scripts the Papyrus VM runs receive this frame's
    // cell-load, activation and trigger events (all produced above). Ahead
    // of quest fragment dispatch, so a stage they set runs its fragment
    // this frame.
    //
    // M47.2 — the alias scripts of quests that run go first: checked when a
    // quest first runs, started (OnInit / OnAliasInit) once its aliases are
    // filled, shut down when it stops. The events of the references the
    // aliases are filled with are delivered by the system after them, with the
    // reference scripts'.
    scheduler.add_exclusive(Stage::Update, check_alias_scripts_dispatch);
    scheduler.add_exclusive(Stage::Update, byroredux_scripting::alias_script_event_system);
    scheduler.add_exclusive(
        Stage::Update,
        byroredux_scripting::reference_script_event_system,
    );
    // M47.2 — activate parents: this frame's activations passed on to the children.
    scheduler.add_exclusive(Stage::Update, byroredux_scripting::activate_parent_relay_system);
    // M47.2 — the object graphs' raised events, delivered as `OnAnimationEvent`
    // to the scripts that registered for them.
    scheduler.add_exclusive(Stage::Update, byroredux_scripting::animation_event_system);
    // M47.2 — `Sound.Play` requests the scripts above recorded.
    scheduler.add_exclusive(Stage::Update, crate::systems::scripted_sound::scripted_sound_system);
    // Dispatch quest fragments right after the advance that emits the
    // `QuestStageAdvanced` markers, before end-of-frame cleanup drains
    // them (populated live from parsed QUST VMAD fragments, #1739 / `8a70b81a`).
    scheduler.add_exclusive(Stage::Update, quest_fragment_dispatch);
    // QSDT Complete/Fail Quest flags and NAM0 successor quests apply after
    // the stage's own fragment has observed the transition.
    scheduler.add_exclusive(
        Stage::Update,
        byroredux_scripting::quest_terminal_stage_system,
    );
    // Latent Utility.Wait tails resume after their authored delay. Running
    // immediately after fresh fragment dispatch lets one timing path serve
    // both newly-suspended and already-pending continuations.
    scheduler.add_exclusive(
        Stage::Update,
        byroredux_scripting::fragment_continuation_system,
    );
    // OnUpdate / OnUpdateGameTime for the scripts registered for them,
    // beside the other latent-script clock.
    scheduler.add_exclusive(Stage::Update, script_update_dispatch);
    // M47.2 — the distance events whose pair is now close (or far).
    scheduler.add_exclusive(Stage::Update, byroredux_scripting::distance_event_system);
    // Translate the resulting Skyrim PlayIdle FormID requests into decoded
    // HKX AnimationPlayers. The parallel animation batch observes a request
    // no later than the following frame, preserving deterministic restarts.
    scheduler.add_exclusive(Stage::Update, crate::systems::idle_clip_playback_system);
    // A one-shot `PlayIdle` that has run its length gives the actor its earlier animation back.
    scheduler.add_exclusive(Stage::Update, crate::systems::idle_resume_system);
    // `PickIdle`: the actor starts its generic idle again.
    scheduler.add_exclusive(Stage::Update, crate::systems::pick_idle_system);
    // Apply Papyrus SetMotionType requests after both immediate and resumed
    // fragment effects, before the Physics stage consumes body state.
    scheduler.add_exclusive(Stage::Update, crate::systems::scripted_motion_type_system);
    // ApplyHavokImpulse requests, after the motion-type change that made the body dynamic.
    scheduler.add_exclusive(Stage::Update, crate::systems::scripted_havok_impulse_system);
    // Fallout 4 actors stand in the idle their animation archetype names; a script's
    // `ChangeAnimArchetype` (run above) swaps it.
    scheduler.add_exclusive(Stage::Update, crate::systems::archetype_idle::archetype_idle_system);
    // M47.2 — `Weapon.Fire` projectiles: start the requested ones, then fly them
    // (a hit on a live actor queues the `HitEvent` the damage system consumes).
    scheduler.add_exclusive(Stage::Update, crate::systems::trap_hits::trap_contact_system);
    scheduler.add_exclusive(Stage::Update, crate::systems::projectiles::projectile_launch_system);
    scheduler.add_exclusive(Stage::Update, crate::systems::projectiles::projectile_flight_system);
    // `HitEvent` is a one-frame marker that `event_cleanup_system` drains in Late, so its
    // consumers run after EVERY producer: the melee / NPC strikes above, the VM effect flush
    // (`ProcessTrapHit`) and the projectile flight just before. (A consumer ahead of a
    // producer never sees its hits: the dart trap's darts dealt no damage that way.)
    // #5872 — `ProcessTrapHit` requests (queued by every script flush, including the Late
    // dialogue one, whose same-frame `HitEvent` would be drained unread) land as `HitEvent`s
    // here: after the script systems and projectile flight, before every consumer.
    scheduler.add_exclusive(Stage::Update, byroredux_scripting::pending_trap_hit_flush_system);
    // M47.2 — the hits scripts registered for (Fallout 4 / Starfield
    // `RegisterForHitEvent`), delivered once each while the `HitEvent` markers
    // stand.
    scheduler.add_exclusive(Stage::Update, byroredux_scripting::hit_event_system);
    // M47.3 — the same markers for the ObScript `OnHit` / `OnHitWith` blocks of a struck
    // actor's script, and the combat transitions for `OnStartCombat` / `OnCombatEnd`.
    // Here rather than beside `obscript_object_system`: the projectile and trap hits
    // above are produced after that one runs.
    scheduler.add_exclusive(Stage::Update, byroredux_scripting::obscript_objects::obscript_hit_system);
    scheduler.add_exclusive(Stage::Update, byroredux_scripting::obscript_objects::obscript_combat_system);
    // M47.3 — the magic effects running on actors: `ScriptEffectStart` / `Update` / `Finish`, value
    // changes over time, and the `OnMagicEffectHit` they raised when they landed. After the ObScript
    // systems whose `Cast` / `CastImmediateOnSelf` start them.
    scheduler.add_exclusive(Stage::Update, byroredux_scripting::magic_effects::magic_effect_system);
    scheduler.add_exclusive(Stage::Update, byroredux_scripting::magic_effects::magic_hit_system);
    // M47.3 — magic shaders: advance their clocks, then glow (or restore) the surfaces they are on.
    scheduler.add_exclusive(Stage::Update, byroredux_scripting::magic_shaders::magic_shader_clock_system);
    scheduler.add_exclusive(Stage::Update, crate::systems::magic_shader::magic_shader_tint_system);
    // M47.3 — `OnDestructionStageChange` for the objects a `DamageObject` moved to another stage.
    scheduler.add_exclusive(Stage::Update, byroredux_scripting::obscript_objects::obscript_destruction_system);
    // M47.3 — `Resurrect`: bring the actors back that scripts named this frame.
    scheduler.add_exclusive(Stage::Update, crate::systems::revive::revive_system);
    // M47.3 — `PushActorAway` (knock the actor down, then stand it up where it came to rest), `ForceFlee`,
    // `UseWeapon`, `ResetInventory`, `SetPCSleepHours`, `PlayMusic` and `EssentialDeathReload`: the app's
    // half of the commands that raise a request.
    scheduler.add_exclusive(Stage::Update, crate::systems::knockdown::push_actor_system);
    scheduler.add_exclusive(Stage::Update, crate::systems::knockdown::get_up_system);
    scheduler.add_exclusive(Stage::Update, crate::systems::flee::flee_system);
    scheduler.add_exclusive(Stage::Update, crate::systems::script_requests::use_weapon_system);
    scheduler.add_exclusive(Stage::Update, crate::systems::script_requests::reset_inventory_system);
    scheduler.add_exclusive(Stage::Update, crate::systems::script_requests::sleep_system);
    scheduler.add_exclusive(Stage::Update, crate::systems::script_requests::music_system);
    scheduler.add_exclusive(Stage::Update, crate::systems::script_requests::reload_system);
    scheduler.add_exclusive(Stage::Update, crate::combat::chargen_catch_up_system);
    scheduler.add_exclusive(Stage::Update, crate::systems::script_requests::script_ui_request_system);
    scheduler.add_exclusive(Stage::Update, crate::systems::script_requests::actor_alpha_system);
    // M47.3 — the New Vegas challenges whose stat has risen far enough complete (reward script and all).
    scheduler.add_exclusive(Stage::Update, byroredux_scripting::challenges::challenge_system);
    scheduler.add_exclusive_with_access(
        Stage::Update,
        crate::combat::combat_damage_system,
        Access::new()
            .reads::<byroredux_core::ecs::components::Inventory>()
            .writes::<byroredux_core::ecs::components::EquipmentSlots>()
            .writes::<byroredux_core::ecs::components::EquippedWeapon>()
            .writes_resource::<crate::combat::CombatState>()
            .reads::<byroredux_scripting::HitEvent>()
            .reads::<byroredux_core::ecs::components::ActorVitals>()
            .reads::<crate::components::AnimationTarget>()
            .writes::<byroredux_core::ecs::components::ActorValues>()
            .writes::<byroredux_core::ecs::components::Dead>()
            // M47.2 — spare_from_death: essential / protected actors bleed out.
            .reads::<byroredux_scripting::ActorProtection>()
            .reads::<byroredux_scripting::SceneAliasCandidate>()
            .reads_resource::<byroredux_scripting::ActorBaseProtectionOverrides>()
            .writes::<byroredux_scripting::Bleedout>()
            .reads_resource::<byroredux_scripting::BleedoutRules>()
            .reads_resource::<byroredux_scripting::SceneQuestAliasRegistry>()
            .reads_resource::<byroredux_scripting::SceneActorBindings>()
            .reads_resource::<byroredux_scripting::quest_stages::QuestStageState>()
            // M47.2 — reconcile_dead_actor's note_actor_death (cleared locations).
            .reads_resource::<byroredux_scripting::LocationTable>()
            .writes_resource::<byroredux_scripting::LocationClearState>()
            .writes::<byroredux_core::animation::AnimationPlayer>()
            .writes::<byroredux_core::animation::AnimationStack>()
            // #5861 / #5870 — reconcile_dead_actor drops the corpse's behavior
            // graph runtime and reports its package ending.
            .writes::<byroredux_scripting::ObjectAnimationGraph>()
            .writes::<crate::systems::GraphLocomotion>()
            .writes::<crate::systems::ConsoleWalk>()
            .writes_resource::<byroredux_scripting::ActivePackages>()
            .writes_resource::<byroredux_scripting::PendingPackageTransitions>(),
    );
    // Animated props' behavior graphs: clip-trigger events move their state machines on.
    scheduler.add_exclusive(Stage::Update, byroredux_scripting::object_graph_step_system);
    // Native cart tethers consume the horse ACHR's XLKR marker chain before
    // the attachment pass copies the resulting pose to carts and riders.
    scheduler.add_exclusive(Stage::Update, crate::systems::cinematic_horse_route_system);
    // Resolve horse -> tethered cart -> SetVehicle rider poses before the
    // PostUpdate transform pass propagates the chain through actor skeletons.
    scheduler.add_exclusive(Stage::Update, crate::systems::vehicle_attachment_system);
    scheduler.add_exclusive(Stage::Update, dlc2_ttr4a_on_init_dispatch);
    // #5842 — the demo dispatchers below (dlc2_ttr4a, mg07) and
    // `recurring_update_tick_system` run every frame with nothing to do
    // in production: nothing spawns their components (they are test
    // surface / living evidence per scripting.md's R5 section), and no
    // recognizer emits `RecurringUpdate`. They stay scheduler-wired so
    // the demos' fixture tests drive the real schedule shape.
    //
    // `recurring_update_tick_system` ticks `RecurringUpdate` and emits
    // `OnUpdateEvent`. It sits between the demo's OnInit (which
    // subscribes via `RegisterForUpdate`) and its OnUpdate consumer so
    // a fired event is handled the same frame, before cleanup drains it.
    scheduler.add_exclusive(
        Stage::Update,
        byroredux_scripting::recurring_update_tick_system,
    );
    // #5039 — refresh the player's stamped PlayerOnly pools from their
    // current inputs before regen reads them. Exclusive, like its neighbour
    // below: it takes the `CharacterRuleset` → `ActorValues` pair.
    scheduler.add_exclusive_with_access(
        Stage::Update,
        crate::systems::player_derived_stats_system,
        Access::new()
            .reads_resource::<crate::systems::PlayerEntity>()
            .reads::<byroredux_core::character::CharacterLevel>()
            .reads_resource::<byroredux_core::character::CharacterRuleset>()
            .writes::<byroredux_core::ecs::components::ActorValues>(),
    );
    // CHARAL pool regen (Fatigue/Magicka) — a fixed 60 Hz tick decoupled
    // from the variable frame rate, mirroring `physics_sync_system`'s
    // accumulator (`crates/core/src/character/regen.rs`).
    //
    // No-ops today, gated on TWO resources (#2950): `PoolRegenAccumulator`,
    // now inserted unconditionally in `build_world` above, and
    // `PoolRegenConfig`, which is per-game and arrives only once a live
    // `CharacterRuleset` wiring reaches Oblivion (`build_character_ruleset`
    // returns `None` for it today, per `npc_spawn.rs`). With the accumulator
    // armed here, the config insertion really is the only thing left — which
    // is what this comment used to claim while a second, undocumented gate
    // sat one line below the first inside the system.
    // #2391 / ECS-D5B-03 — declared via `add_exclusive_with_access` (the
    // #1236 channel, previously unused in production). This system is
    // #2153's site: it builds a 3-deep hold stack (`PoolRegenConfig` read
    // held across `PoolRegenAccumulator` write, then `CharacterRuleset`
    // read, then the `ActorValues` write query), and its safety rests
    // entirely on being scheduled exclusive. The analyzer doesn't pair
    // exclusives, so this declaration surfaces no conflict row — its job
    // is to put the disputed types on the `sys.accesses` report instead
    // of leaving a blank row where the dispute actually is.
    scheduler.add_exclusive_with_access(
        Stage::Update,
        byroredux_core::character::pool_regen_tick_system,
        Access::new()
            .reads_resource::<byroredux_core::character::PoolRegenConfig>()
            .writes_resource::<byroredux_core::character::PoolRegenAccumulator>()
            .reads_resource::<byroredux_core::character::CharacterRuleset>()
            .reads::<byroredux_core::character::CharacterLevel>()
            .writes::<byroredux_core::ecs::components::ActorValues>(),
    );
    scheduler.add_exclusive(Stage::Update, dlc2_ttr4a_on_update_dispatch);
    scheduler.add_exclusive(Stage::Update, mg07_on_load_dispatch);
    scheduler.add_exclusive(Stage::Update, mg07_on_activate_dispatch);
    scheduler.add_exclusive(
        Stage::Update,
        byroredux_scripting::papyrus_demo::mg07_door::mg07_tick_system,
    );
    scheduler.add_to_with_access(
        Stage::Update,
        make_animation_system(),
        // M27 — animation_system writes the full set of animated-
        // channel storages (every channel a clip may target). The
        // declaration is the UNION across all paths; individual
        // frames touch a subset depending on which clips are
        // playing. See `byroredux/src/systems/animation.rs:305`.
        Access::new()
            .reads_resource::<byroredux_core::animation::AnimationClipRegistry>()
            .writes_resource::<byroredux_core::animation::AnimationClipRegistry>()
            .reads_resource::<crate::components::SubtreeCache>()
            .writes_resource::<crate::components::SubtreeCache>()
            .reads_resource::<crate::components::NameIndex>()
            .writes_resource::<crate::components::NameIndex>()
            .writes_resource::<byroredux_core::string::StringPool>()
            .reads::<byroredux_core::ecs::Name>()
            .reads::<byroredux_core::ecs::Children>()
            .writes::<Transform>()
            .writes::<byroredux_core::animation::RootMotionDelta>()
            .writes::<byroredux_core::ecs::AnimatedVisibility>()
            .writes::<byroredux_core::ecs::AnimatedDiffuseColor>()
            .writes::<byroredux_core::ecs::AnimatedAmbientColor>()
            .writes::<byroredux_core::ecs::AnimatedSpecularColor>()
            .writes::<byroredux_core::ecs::AnimatedEmissiveColor>()
            .writes::<byroredux_core::ecs::AnimatedShaderColor>()
            .writes::<byroredux_core::ecs::AnimatedAlpha>()
            .writes::<byroredux_core::ecs::AnimatedUvTransform>()
            .writes::<byroredux_core::ecs::AnimatedShaderFloat>()
            .writes::<byroredux_core::ecs::AnimatedMorphWeights>()
            .writes::<byroredux_core::ecs::AnimatedTextureFlip>()
            .writes::<byroredux_core::ecs::LightSource>()
            .writes::<byroredux_core::animation::AnimationPlayer>()
            .writes::<byroredux_scripting::events::AnimationTextKeyEvents>()
            .writes::<byroredux_core::animation::AnimationStack>(),
    );
    // Translate clip text keys into Skyrim behavior/Papyrus completion
    // notifications in the same frame, before Late drains the transient
    // AnimationTextKeyEvents components.
    scheduler.add_exclusive(Stage::Update, crate::systems::cinematic_root_motion_system);
    // M47.2 — a graph-driven actor is carried by the root motion its clips extract.
    scheduler.add_exclusive(Stage::Update, crate::systems::graph_root_motion_system);
    // #2391 — the second half of #2269's inverted pair. The body reads
    // text-key events and writes `ActorCinematicState`, then hands the
    // player's events to `dispatch_player_cinematic_animation_event`,
    // which takes `CinematicPresentationState` and then `QuestStageState`
    // — the reverse of the order `quest_fragment_dispatch` establishes.
    // That inversion is safe only because both systems are exclusive;
    // naming both resources here is what makes the contract legible in
    // `sys.accesses` rather than only in prose.
    scheduler.add_exclusive_with_access(
        Stage::Update,
        crate::systems::cinematic_animation_event_system,
        Access::new()
            .reads_resource::<StringPool>()
            .reads_resource::<byroredux_scripting::papyrus_demo::PapyrusPlayerEntity>()
            .writes_resource::<byroredux_scripting::CinematicPresentationState>()
            .writes_resource::<byroredux_scripting::quest_stages::QuestStageState>()
            .reads::<byroredux_scripting::AnimationTextKeyEvents>()
            .writes::<byroredux_scripting::ActorCinematicState>(),
    );
    // Sample freshly delivered ImageSpaceModifier.Apply callbacks after the
    // animation event system so MQ101's blur begins in this rendered frame.
    scheduler.add_exclusive(
        Stage::Update,
        byroredux_scripting::image_space_modifier_system,
    );
    // M27 Phase 3 — `spin_system` writes Transform on entities
    // tagged with `Spinning` (the demo cube). `animation_system`
    // also writes Transform on its own (disjoint) entity set.
    // They never touch the same entity, but the analyzer can't see
    // that — they pair as a WriteWrite Transform conflict. Moving
    // `spin_system` to exclusive sequences it after the Update
    // parallel batch and removes the conflict from the report
    // without changing observable behaviour. Cost: ~µs of lost
    // parallelism on the demo cube; negligible.
    scheduler.add_exclusive(Stage::Update, spin_system);
    // Creation-era loading-screen turntable — same shape as spin_system:
    // writes Transform on its own (marker-gated, at most one entity) set,
    // sequenced after the Update batch to keep the analyzer's
    // Transform WriteWrite row clean.
    scheduler.add_exclusive(Stage::Update, loading_model_turntable_system);
    // Phase 17 — procedural light flicker. Writes
    // LightSource.intensity + Transform.translation on entities
    // with a LightFlicker companion. Exclusive in Update so it
    // sequences AFTER the parallel batch (no Transform conflict
    // with animation_system / spin_system) but BEFORE
    // PostUpdate's transform propagation reads the result.
    scheduler.add_exclusive(Stage::Update, animate_lights_system);
    // M47.3 — ObScript `Look`: turn the head bones of actors told to head-track, on top of the pose the
    // animation batch just wrote and before propagation reads it.
    scheduler.add_exclusive(Stage::Update, crate::systems::head_look::head_look_system);
    // P3 player body — third-person facing. Writes the body root's Transform
    // rotation from `InputState.yaw`, the same look accumulator
    // `camera_follow_system` reads. #4995 — registered in **Update**, not
    // Late: its only input is written between frames (winit input, final
    // before the scheduler runs), so nothing forces it after Physics, and
    // its only consumer is PostUpdate transform propagation. In Late it
    // was composed a frame after the camera used the same yaw, so the body
    // visibly trailed the view on fast turns. Exclusive: the body-root
    // Transform write would pair WriteWrite with the animation batch in the
    // analyzer, and sequencing after that batch keeps the facing
    // authoritative over any animated root rotation. Pinned by
    // `player_body_facing_runs_in_update_before_propagation`.
    scheduler.add_exclusive_with_access(
        Stage::Update,
        crate::player_body::player_body_facing_system,
        Access::new()
            .reads_resource::<crate::systems::PlayerMode>()
            .reads_resource::<crate::player_body::PlayerBodyRootEntity>()
            .reads_resource::<crate::components::InputState>()
            .reads::<crate::player_body::PlayerBodyRoot>()
            .writes::<Transform>(),
    );
}
