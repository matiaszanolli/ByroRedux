//! Scripting subsystem — ECS-native event system.
//!
//! Replaces the Papyrus VM with native ECS patterns:
//! - Script state → component fields
//! - Script logic → ECS systems
//! - Events → transient marker components
//! - Timers → ScriptTimer component + tick system
//!
//! The core lifecycle: event marker appears → systems process it →
//! cleanup removes it at end of frame.

pub mod cinematic;
pub mod cleanup;
pub mod condition;
pub mod dialogue;
pub mod equipment;
pub mod events;
pub mod fragment;
pub mod globals;
pub mod package;
pub mod papyrus_demo;
pub mod player_control;
pub mod quest_stages;
pub mod recurring_update;
pub mod registry;
pub mod scene;
pub mod timer;
pub mod translate;
pub mod trigger;
pub mod vm_state;

pub use cinematic::{
    dispatch_player_cinematic_animation_event, image_space_modifier_system,
    install_image_space_modifiers, ActorCinematicState, CinematicAnimationEvent,
    CinematicPresentationState, HorseTetherState, ImageSpaceModifierApplication,
    ImageSpaceModifierFrame, MotionTypeChangeRequest,
};
pub use cleanup::event_cleanup_system;
pub use condition::{
    evaluate as evaluate_condition_list, evaluate_condition, evaluate_function, ConditionContext,
    ConditionFunction,
};
pub use dialogue::{
    estimate_dialogue_duration, install_dialogue_records, scene_dialogue_system,
    ActiveDialogueLine, DialogueLine, DialogueLineCompletionBatch, DialoguePlayback,
    DialoguePresentationEvent, DialoguePresentationEventBatch, DialogueRegistry,
};
pub use equipment::{install_equip_item_catalog, EquipItemCatalog};
pub use events::{
    ActivateEvent, AnimationTextKeyEvent, AnimationTextKeyEvents, HitEvent, OnCellLoadEvent,
    OnEquipEvent, OnTriggerEnterEvent, RippleEvent, SplashEvent, TimerExpired,
};
pub use fragment::{
    apply_effects, fragment_activation_flush_system, fragment_continuation_system,
    populate_quest_fragments_from_pex, populate_scene_fragments_from_pex,
    quest_fragment_dispatch_system, scene_fragment_dispatch_system, DeferredFragmentEffects,
    FragmentExecutionQueue, PendingFragmentActivations, QuestStageFragments, SceneFragments,
};
pub use globals::Globals;
pub use package::{
    install_package_records, install_package_target_positions, scene_package_system,
    ActiveScenePackageAction, EvaluatePackageRequest, PackageRegistry, PackageTargetRegistry,
    ScenePackageCommand, ScenePackageCompletionBatch, ScenePackageEvent, ScenePackageEventBatch,
    ScenePackagePlayback,
};
pub use player_control::{ActorControlState, PlayerControlSelection, PlayerControlState};
pub use quest_stages::{
    install_engine_start_quest, install_quest_alias_readiness_gate, install_start_game_quests,
    quest_alias_readiness_stage_system, quest_startup_system, quest_terminal_stage_system,
    resolve_quest_objective_targets, resolve_quest_targets, QuestAliasReadinessGate,
    QuestDefinitionRegistry, QuestEventRead, QuestEventSubscriberId, QuestFormId, QuestStatus,
    SequencedQuestStageAdvanced, StartGameQuestRegistry, QUEST_EVENT_RETENTION,
};
pub use recurring_update::{recurring_update_tick_system, OnUpdateEvent, RecurringUpdate};
pub use registry::{ScriptRegistry, ScriptSpawnFn};
pub use scene::{
    install_scene_quest_aliases, install_scene_records, mark_scene_actor_bindings_dirty,
    quest_alias_diagnostics, quest_alias_refresh_system, refresh_scene_actor_bindings,
    scene_playback_system, ActiveSceneAction, QuestAliasDiagnostic, QuestAliasInjectedOverlays,
    QuestAliasInjectionState, QuestAliasResolutionState, QuestAliasRuntimeOverlays,
    SceneActionCompletionBatch, SceneActorBindings, SceneAliasCandidate, SceneEvent,
    SceneEventBatch, SceneFragmentInvocation, SceneFragmentInvocationBatch, ScenePlaybackState,
    ScenePlayer, SceneRegistry, SceneStartRequest, SceneStopRequest,
};
pub use timer::{timer_tick_system, ScriptTimer};
pub use translate::{
    translate_pex, translate_script, CanonicalEvent, RecognizeCtx, Recognized, ScriptSource,
};
pub use trigger::{trigger_detection_system, TriggerShape, TriggerVolume};
pub use vm_state::{
    two_state_activator_system, ScriptVariables, TwoStateActivator, TwoStateTransition,
    TwoStateTransitionBatch,
};

use byroredux_core::ecs::world::World;

/// Register all scripting component storages in the world.
///
/// Call during setup so that `query_mut()` works for event markers
/// before any entity has triggered an event.
pub fn register(world: &mut World) {
    world.register::<ActivateEvent>();
    world.register::<HitEvent>();
    world.register::<SplashEvent>();
    world.register::<RippleEvent>();
    world.register::<TimerExpired>();
    world.register::<AnimationTextKeyEvents>();
    world.register::<ScriptTimer>();
    // M47.0 Phase 5 — canonical event markers. OnCellLoadEvent +
    // OnTriggerEnterEvent + OnEquipEvent join the existing
    // ActivateEvent / HitEvent / TimerExpired in the script-event
    // catalog. Emit sites land per-phase:
    //   * OnCellLoadEvent — emitted by the cell loader's
    //     `attach_script_for_refr` (Phase 5).
    //   * OnTriggerEnterEvent — emitted by `trigger_detection_system`
    //     on player entry (M47.2).
    //   * OnEquipEvent — deferred to M41 equip pipeline integration.
    // All three are one-frame transients drained by `event_cleanup_system`.
    world.register::<OnCellLoadEvent>();
    world.register::<OnTriggerEnterEvent>();
    world.register::<OnEquipEvent>();
    // M47.2 — trigger-volume storage. The cell loader attaches a
    // `TriggerVolume` to each invisible trigger REFR; `trigger_detection_system`
    // emits `OnTriggerEnterEvent` on player entry, which the quest-advance
    // dispatch consumes (the `default*Trigger` family).
    trigger::register(world);
    recurring_update::register(world);
    quest_stages::register(world);
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
