//! Player-derived pools, recurring update ticks, the animation batch, cinematics, spin / turntable / lights, head look and the player body facing tail.
//!
//! A contiguous run of `Stage::Update` registrations (#6359): registration order is the
//! contract, so [`super::register_update_systems`] calls the segments in file order.

use super::*;

/// Player-derived pools, recurring update ticks, the animation batch, cinematics, spin / turntable / lights, head look and the player body facing tail.
pub(super) fn register_player_and_animation(scheduler: &mut Scheduler) {
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
            // #6288 / #6289 — death ends the actor's magic effects and its forced head-look.
            .writes::<byroredux_scripting::magic_effects::ActiveMagicEffects>()
            .writes::<byroredux_scripting::obscript_anim::HeadLook>()
            .writes_resource::<byroredux_scripting::ActivePackages>()
            .writes_resource::<byroredux_scripting::PendingPackageTransitions>()
            // `crime::report_strike` — the assault / murder bounty, its witnesses, and the
            // hostility judgement of the victim.
            .reads_resource::<crate::systems::PlayerEntity>()
            .reads_resource::<byroredux_core::character::CrimeModel>()
            .writes_resource::<byroredux_scripting::obscript_ledger::ObScriptLedger>()
            .writes_resource::<crate::notifications::PlayerNotifications>()
            .writes::<crate::crime::AssaultedByPlayer>()
            // `crime::raise_alarm` / `record_crime` — who saw a crime, and the guards a witness calls.
            .writes_resource::<byroredux_scripting::crime_knowledge::CrimeKnowledge>()
            .reads_resource::<byroredux_scripting::guards::GuardClasses>()
            .reads::<byroredux_core::character::Background>()
            .writes::<crate::systems::forcegreet::ForceGreetDirective>()
            // `crime::charge_crime_factions` — Skyrim's per-hold bounty: the witnesses' crime factions.
            .reads_resource::<byroredux_scripting::crime_faction::CrimeFactionData>()
            .reads_resource::<byroredux_scripting::CrimeFactions>()
            .reads_resource::<byroredux_scripting::CrimeFactionOverrides>()
            .writes_resource::<byroredux_scripting::FactionCrimeGold>()
            .reads_resource::<crate::cell_loader::LoadedCellIndex>()
            .reads::<byroredux_scripting::AiCombatState>()
            .reads::<byroredux_core::ecs::components::FormIdComponent>()
            .reads_resource::<byroredux_core::form_id::FormIdPool>()
            .reads::<byroredux_core::ecs::components::CreatureActor>()
            .reads::<byroredux_core::ecs::components::FactionRanks>()
            .reads::<byroredux_core::ecs::components::GlobalTransform>()
            .reads::<byroredux_scripting::AiCombatState>()
            .reads::<crate::systems::faction_hostility::CombatDisposition>()
            .reads_resource::<crate::systems::faction_hostility::DetectionConfig>()
            .reads_resource::<crate::components::CellLightingRes>()
            .reads_resource::<byroredux_scripting::FactionRelations>()
            .reads_resource::<byroredux_scripting::LoadOrderIdentity>()
            .reads_resource::<crate::cell_loader::load_order::GlobalFormIdResolver>()
            .reads_resource::<byroredux_physics::PhysicsWorld>()
            .reads::<byroredux_physics::RapierHandles>(),
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
    // Gated on TWO resources (#2950): `PoolRegenAccumulator`, inserted
    // unconditionally in `build_world`, and `PoolRegenConfig`, which is
    // per-game. FO3/FNV install it with an `ActionPoints` lane (#6350,
    // `build_vats_model`), so the tick restores the player's AP there; every
    // other game never inserts it and the system returns at its second line
    // (Oblivion's Fatigue/Magicka lanes wait on a pre-AVIF resolver, #3768).
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
            .writes::<byroredux_core::animation::RootMotionYaw>()
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
    // #4128 — ObScript `PlayGroup <group> 0` ("let the current cycle finish
    // first"): start the held sequence at the cycle boundary the animation
    // batch just crossed, on the same exclusive tail so the wrap is visible.
    fn pending_group_swap_dispatch(world: &World, _dt: f32) {
        byroredux_scripting::obscript_anim::apply_pending_group_swaps(world)
    }
    scheduler.add_exclusive(Stage::Update, pending_group_swap_dispatch);
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
