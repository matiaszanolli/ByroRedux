//! `Stage::Update` registrations (#3855, split from `boot.rs`).
//!
//! The largest stage, and the one carrying the nested dispatch shims that
//! adapt per-game runtimes to the scheduler's `fn(&World, f32)` shape.

use byroredux_core::ecs::{Access, ActiveCamera, Scheduler, Stage, Transform, World};
use byroredux_core::string::StringPool;

use crate::interaction::{
    ActionState, InteractionCandidateScratch, InteractionState, InteractionTrace,
};
use crate::systems::{animate_lights_system, make_animation_system, spin_system};

/// `Stage::Update` registrations (#3739 split of `build_scheduler`).
pub(super) fn register_update_systems(scheduler: &mut Scheduler) {
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
            .reads_resource::<ActionState>()
            .reads_resource::<ActiveCamera>()
            .reads_resource::<byroredux_physics::PhysicsWorld>()
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
            .writes::<byroredux_scripting::ActivateEvent>(),
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
            .writes::<byroredux_scripting::HitEvent>(),
    );
    scheduler.add_exclusive_with_access(
        Stage::Update,
        crate::combat::combat_damage_system,
        Access::new()
            .writes_resource::<crate::combat::CombatState>()
            .reads::<byroredux_scripting::HitEvent>()
            .reads::<byroredux_core::ecs::components::ActorVitals>()
            .reads::<crate::components::HavokAnimationTarget>()
            .writes::<byroredux_core::ecs::components::ActorValues>()
            .writes::<byroredux_core::ecs::components::Dead>()
            .writes::<byroredux_core::animation::AnimationPlayer>(),
    );
    // #2654 — quest fragments queue their `<Ref>.Activate()` targets rather
    // than inserting `ActivateEvent` directly, because
    // `quest_fragment_dispatch` runs *after* three of the four consumers
    // below (it has to: it consumes the `QuestStageAdvanced` markers
    // `quest_advance_dispatch` emits) and `event_cleanup_system` drains the
    // marker at Stage::Late the same frame. Flush the queue here, alongside
    // the canonical player-interaction producer, so a fragment activation
    // reaches every consumer exactly once on the following frame.
    scheduler.add_exclusive(
        Stage::Update,
        byroredux_scripting::fragment_activation_flush_system,
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
    // PACK actions resolve their Skyrim PKCU template/data inputs, move actors
    // toward authored invisible-marker coordinates for Travel-family leaves,
    // and queue Done completions for scene playback's next tick.
    scheduler.add_exclusive(Stage::Update, byroredux_scripting::scene_package_system);
    // Package Activate leaves emit the same canonical OnActivate marker as
    // player interaction — but they QUEUE it through
    // `PendingFragmentActivations` rather than inserting it here (#3936),
    // because `scene_package_system` above runs after
    // `rumble_on_activate_dispatch` and `quest_advance_dispatch`. The
    // head-of-frame `fragment_activation_flush_system` delivers it to all
    // four consumers next frame. This registration consumes the flushed
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
    // Translate the resulting Skyrim PlayIdle FormID requests into decoded
    // HKX AnimationPlayers. The parallel animation batch observes a request
    // no later than the following frame, preserving deterministic restarts.
    scheduler.add_exclusive(Stage::Update, crate::systems::havok_idle_playback_system);
    // Apply Papyrus SetMotionType requests after both immediate and resumed
    // fragment effects, before the Physics stage consumes body state.
    scheduler.add_exclusive(Stage::Update, crate::systems::scripted_motion_type_system);
    // Native cart tethers consume the horse ACHR's XLKR marker chain before
    // the attachment pass copies the resulting pose to carts and riders.
    scheduler.add_exclusive(Stage::Update, crate::systems::cinematic_horse_route_system);
    // Resolve horse -> tethered cart -> SetVehicle rider poses before the
    // PostUpdate transform pass propagates the chain through actor skeletons.
    scheduler.add_exclusive(Stage::Update, crate::systems::vehicle_attachment_system);
    scheduler.add_exclusive(Stage::Update, dlc2_ttr4a_on_init_dispatch);
    // `recurring_update_tick_system` ticks `RecurringUpdate` and emits
    // `OnUpdateEvent`. It sits between the demo's OnInit (which
    // subscribes via `RegisterForUpdate`) and its OnUpdate consumer so
    // a fired event is handled the same frame, before cleanup drains it.
    scheduler.add_exclusive(
        Stage::Update,
        byroredux_scripting::recurring_update_tick_system,
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
    // Phase 17 — procedural light flicker. Writes
    // LightSource.intensity + Transform.translation on entities
    // with a LightFlicker companion. Exclusive in Update so it
    // sequences AFTER the parallel batch (no Transform conflict
    // with animation_system / spin_system) but BEFORE
    // PostUpdate's transform propagation reads the result.
    scheduler.add_exclusive(Stage::Update, animate_lights_system);
}
