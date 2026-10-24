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

pub mod actor_value_names;
pub mod alias_overrides;
pub mod bleedout;
pub mod cell_kind;
pub mod cinematic;
pub mod cleanup;
pub mod combat;
pub mod compatibility;
pub mod condition;
mod registration;

pub use registration::register;
pub mod editor_location;
pub mod created_references;
pub mod animation_graph;
mod animation_modifiers;
pub mod bribe;
pub mod havok_impulse;
pub mod condition_form;
pub mod dialogue;
pub mod encounter_level;
pub mod equipment;
pub mod events;
pub mod faction_changes;
pub mod fragment;
pub mod globals;
pub mod idle_names;
pub mod load_order;
pub mod map_markers;
pub mod activate_parents;
pub mod anim_subgraph;
pub mod destruction;
pub mod detection_events;
pub mod keywords;
pub mod menus;
pub mod alias_collections;
pub mod followers;
pub mod form_lists;
pub mod input_layers;
pub mod actor_base_traits;
pub mod crime_faction;
pub mod crime_gold;
pub mod linked_refs;
pub mod loc_ref_types;
pub mod reference_flags;
pub mod projectile_requests;
pub mod sound_requests;
pub mod trap_hit_requests;
pub mod ownership;
pub mod crime_knowledge;
pub mod guards;
pub mod jail;
pub mod weather_state;
pub mod location_keyword_data;
pub mod messages;
pub mod name_table;
pub mod scanning;
pub mod skills;
pub mod speechcraft;
pub mod misc_stats;
pub mod location;
pub mod magic;
pub mod magic_effects;
pub mod magic_shaders;
pub mod obscript;
mod obscript_actions;
pub mod obscript_actor_values;
mod obscript_effects;
pub mod challenges;
pub mod obscript_ledger;
pub mod obscript_more;
pub mod obscript_anim;
pub mod obscript_speech;
pub mod obscript_objects;
pub mod obscript_quests;
pub mod obscript_results;
#[cfg(feature = "save")]
pub mod serde_pairs;
pub mod obscript_state;
pub use obscript_state::{PlayingScriptedIdle, SightQuery, SwimQuery};
pub mod obscript_runtime;
pub mod obscript_vm;
pub mod package;
pub mod script_packages;
pub mod perk_entry_points;
pub mod papyrus_demo;
pub mod papyrus_provider;
pub mod player_control;
pub mod quest_stages;
pub mod quest_variables;
pub mod relationships;
pub mod recurring_update;
pub mod registry;
pub mod scene;
pub mod story_manager;
pub mod timer;
pub mod translate;
pub mod trigger;
pub mod vm_state;

pub use cinematic::{
    dispatch_player_cinematic_animation_event, image_space_modifier_system,
    install_image_space_modifiers, ActorCinematicState, CinematicAnimationEvent, PickIdleRequest,
    CinematicPresentationState, HorseTetherState, ImageSpace, ImageSpaceBase, ImageSpaceModifier,
    ImageSpaceModifierApplication, MotionTypeChangeRequest,
};
pub use cleanup::event_cleanup_system;
pub use story_manager::{
    build_story_manager_tree, emit_change_location_on_key_change, install_story_manager,
    story_manager_dispatch_system, SmNodeRuntime, SmTree, SmTreeNode, StoryClock, StoryEvent,
    StoryEventAliasFill, StoryEventFill, StoryLocationCursor, StoryManagerNodeState, StoryManagerRng,
};
pub use combat::{combined_reaction, faction_reaction, FactionCatalog, is_in_combat, AiCombatState, AmbientEngagement, FactionReactionOverride, FactionRelations};
pub use compatibility::{
    analyze_obscript_bytecode_compatibility, analyze_pex_compatibility,
    analyze_source_compatibility, classify_method_call, classify_static_call,
    record_compatibility_report, source_alias, CompatibilityDisposition, CompatibilityFinding,
    CompatibilityMatch, CompatibilityRegistry, CompatibilityReport, CompatibilitySummaryEntry,
    SourceAlias, SourceCompatibilityFinding, SourceCompatibilityReport,
};
pub use condition::{
    evaluate as evaluate_condition_list, evaluate_condition, evaluate_function, ConditionContext,
    ConditionFunction,
};
pub use dialogue::{
    DialogueSurfaceState,
    estimate_dialogue_duration, install_dialogue_quest_conditions, install_dialogue_records,
    note_info_spoken,
    scene_dialogue_system, select_first_info, ActiveDialogueLine, DialogueLine,
    DialogueLineCompletionBatch, DialoguePlayback, DialoguePresentationEvent,
    DialoguePresentationEventBatch, DialogueQuestConditions, DialogueQuestPriorities,
    DialogueRandomState,
    DialogueRegistry, DialogueSpokenInfoForms,
};
pub use equipment::{
    emit_equipment_changes, emit_item_transfers, install_equip_item_catalog, EquipItemCatalog,
};
pub use events::{
    papyrus_activations, ActivateEvent, NativeActivation, AnimationTextKeyEvent, AnimationTextKeyEvents, EquipmentChange,
    EquipmentEventBatch, HitEvent, ItemEventBatch, ItemTransfer, OnCellLoadEvent, OnDeathEvent, OnInitEvent,
    OnTriggerEnterEvent, OnTriggerLeaveEvent, RippleEvent, SplashEvent, TimerExpired,
};
pub use fragment::{
    add_items, remove_items, apply_effects, apply_spoken_info_fragment, fragment_activation_flush_system, fragment_idle_forms,
    fragment_continuation_system, populate_owned_info_fragments_from_pex_with_providers,
    populate_owned_quest_fragments_from_pex_detailed_with_providers,
    populate_owned_quest_fragments_from_script_with_providers,
    populate_owned_scene_fragments_from_pex_detailed_with_providers,
    populate_owned_scene_fragments_from_script_with_providers, populate_quest_fragments_from_pex,
    populate_quest_fragments_from_pex_detailed,
    populate_scene_fragments_from_pex_detailed, quest_fragment_dispatch_system,
    scene_fragment_dispatch_system, DeferredFragmentEffects, DialogueInfoFragments,
    FragmentExecutionQueue, FragmentPexTranslation, LockOverride, OwnedFragmentProviders,
    PendingFragmentActivations, PlayerMoveRequest, QuestStageFragments, ReferenceActivationBlocks,
    ReferenceEnableState, default_activation_blocked, install_authored_disabled_references,
    reference_enabled, reference_enabled_from, reference_enabled_with_parent,
    AuthoredDisabledReferences, install_quest_script_instances,
    QuestScriptInstances,
    ReferenceLockState, ReferenceMotionTypeState, ReferenceScriptState, SceneFragments,
    attach_reference_script, check_reference_script, reference_script_event_system,
    reference_script_hit_system,
    PapyrusReferenceScripts, ReferenceScript, ReferenceScriptIndex, VmFrameBudget, VmFrameUse, DELIVERED_EVENTS,
    script_update_system, ScriptUpdates, UpdateClock, UpdateRegistration, UpdateRequest,
    EventRegistration, EventRegistrations, EventRequest, REMOTE_EVENTS, distance_event_system, DistanceRequest, DistanceTarget, DistanceWatch, DistanceWatches,
    hit_event_system, HitFilter, HitRegistration, HitRegistrations, HitRequest,
    animation_event_system, AnimationEventRegistration, AnimationEventRegistrations, AnimationEventRequest,
    raise_unload_events, raise_reference_call, run_debug_statement, run_quest_script_function, raise_reference_event, ActorWatch, PendingUnloadDeliveries, discard_pending_unload_deliveries, rebaseline_actor_watch,
    alias_script_event_system, check_alias_script, install_quest_alias_scripts, is_alias_owner, AliasScript,
    AliasScriptOwners, QuestAliasScripts, ALIAS_OWNER_BASE, DELIVERED_ALIAS_EVENTS,
    package_fragment_system, populate_fragments, populate_package_fragments, FragmentSet, FragmentTable, report_package_end, report_package_transition, discard_package_reports, reported_package, retire_active_packages, ActivePackages, PackageEnd,
    PackageFragments, PackageTransition, PendingPackageEnds, PendingPackageTransitions,
    report_terminal_selection, terminal_fragment_system, PendingTerminalSelections, TerminalFragments, TerminalItem, TerminalSelection,
    perk_fragment_system, report_perk_entry, PendingPerkEntries, PerkEntryId, PerkEntryRun, PerkFragments,
};
pub use faction_changes::{
    reapply_scripted_faction_changes, FactionChange, ScriptedFactionChanges, PLAYER_REFERENCE,
};
pub use globals::Globals;
pub use alias_collections::{AliasCollections, CollectionAliasKinds};
pub use editor_location::EditorLocation;
pub use location_keyword_data::{location_keyword_data, LocationKeywordData};
pub use weather_state::{install_weather_classifications, CurrentWeather, WeatherClassifications};
pub use form_lists::{forms_of, install_form_lists, FormListAdditions, FormListChange, FormLists};
pub use animation_graph::{
    object_graph_step_system, AnimArchetypeOverrides, BehaviorGraphRef, GraphClip, GraphSequences, LazyClips, LoadedGraph, ObjectAnimationGraph,
    Playback,
};
pub use scanning::ScannedReferences;
pub use speechcraft::{SpeechcraftOutcome, SpeechcraftOutcomes};
pub use condition_form::{install_condition_forms, is_true as condition_form_is_true, ConditionForms};
pub use havok_impulse::{
    install_havok_impulse_rules, request_impulse as request_havok_impulse, HavokImpulseRequest, HavokImpulseRules,
};
pub use bribe::{bribe_amount, install_bribe_rules, BribeRules};
pub use encounter_level::{
    calculate_encounter_level, install_encounter_level_rules, EncounterLevelRules, ExplicitEncounterZone,
};
pub use ownership::{
    actor_owner, actor_owner_with, owner_override, faction_owner, faction_owner_with, install_owner_kinds, is_faction, ExplicitOwner,
    OwnerKinds, OwnerOverrides,
};
pub use crime_gold::{crime_gold_with, CrimeGoldChange, FactionCrimeGold};
pub use activate_parents::{
    activate_parent_relay_system, install_activate_parent_table, ActivateParentTable, PendingParentActivations,
};
pub use detection_events::{DetectionEvent, PendingDetectionEvents};
pub use projectile_requests::{PendingProjectileLaunches, ProjectileLaunch};
pub use trap_hit_requests::{pending_trap_hit_flush_system, PendingTrapHits, TrapHit};
pub use sound_requests::{install_sound_editor_ids, PendingSoundPlays, SoundEditorIds, SoundPlay};
pub use reference_flags::{ReferenceFlag, ReferenceFlagChange, ReferenceFlags};
pub use input_layers::{Control, InputLayers};
pub use actor_base_traits::{install_actor_base_traits, voice_type, ActorBaseTrait, ActorBaseTraits};
pub use crime_faction::{crime_faction, install_crime_factions, CrimeFactionOverrides, CrimeFactions};
pub use linked_refs::{linked_ref, linked_ref_children, LinkedRefOverrides};
pub use loc_ref_types::{has_loc_ref_type, LocRefTypeOverrides};
pub use skills::{advance_skill, PendingSkillAdvances, SkillProgress, SkillScaling};
pub use followers::{FollowerFlag, FollowerFlags, FollowerState};
pub use misc_stats::{Achievements, MiscStats};
pub use actor_value_names::{install_actor_value_names, ActorValueNames, ScriptGame};
pub use alias_overrides::{AliasOverride, AliasOverrides};
pub use created_references::{
    CreatedCell, CreatedDeletion, CreatedPlacement, CreatedReference, CreatedReferences,
    ScriptedIds, CREATED_FORM_ID_BASE, SCRIPTED_FORM_ID_BASE,
};
pub use load_order::LoadOrderIdentity;
pub use map_markers::{
    install_map_markers, map_marker_fast_travel, map_marker_visible, AuthoredMapMarkers,
    MapMarkers,
};
pub use location::{
    current_location, editor_location, install_location_table, location_cleared,
    note_actor_death, LocationAliasBindings, LocationClearState, LocationTable, PlayerLocation,
};
pub use magic::{
    add_race_spells, add_spell, remove_race_spells, remove_spell, CanonicalSpell,
    ConstantModifier, RaceSpells, SpellCatalog, SpellList,
};
pub use obscript::{
    decode_extender_calls, legacy_load_order_call, ObscriptArgument, ObscriptCall, ObscriptDecode,
    ObscriptDialect, ObscriptLoadOrderCallError,
};
pub use obscript_runtime::{
    attach_legacy_obscript_program, compile_legacy_obscript_bytecode_program,
    compile_legacy_obscript_program, legacy_obscript_load_order_system,
    set_extension_script_function_invoker, set_legacy_obscript_content_catalog,
    ExtensionScriptFunctionCallback, ExtensionScriptFunctionInvoker, LegacyObscriptAssignment,
    LegacyObscriptCall, LegacyObscriptContentCatalog, LegacyObscriptEvent, LegacyObscriptProgram,
    LegacyObscriptStatement,
};
pub use package::{
    install_package_linked_references, install_package_records, install_package_target_directions,
    install_package_target_positions, scene_package_system, ActiveScenePackageAction,
    EvaluatePackageRequest, PackageRegistry, PackageTargetRegistry, ScenePackageCommand,
    ScenePackageCompletionBatch, ScenePackageEvent, ScenePackageEventBatch, ScenePackagePlayback,
};
pub use papyrus_provider::{
    attach_owned_papyrus_provider_program, attach_papyrus_provider_program, lower_provider_call,
    lower_provider_program, papyrus_provider_system, queue_papyrus_mod_event,
    set_papyrus_provider_entity_resolver, set_papyrus_provider_form_resolver,
    set_papyrus_provider_mod_event_publisher, set_papyrus_provider_runtime, PapyrusModEventRuntime,
    PapyrusModEventSender, PapyrusProviderArgument, PapyrusProviderCallback,
    PapyrusProviderCatalog, PapyrusProviderCatalogError, PapyrusProviderCondition,
    PapyrusProviderContinuationQueue, PapyrusProviderEntityResolver, PapyrusProviderEvent,
    PapyrusProviderFormResolver, PapyrusProviderInvocation, PapyrusProviderLowerError,
    PapyrusProviderModEventPublisher, PapyrusProviderProgram, PapyrusProviderProgramError,
    PapyrusProviderRoute, PapyrusProviderRuntime, PapyrusProviderStatement,
    TypedPapyrusProviderCall,
};
pub use relationships::{
    install_relationships, relationship_rank, AuthoredRelationships, ScriptedRelationships,
};
pub use messages::{
    install_message_catalog, MessageCatalog, PendingScriptMessages, ScriptMessage,
};
pub use bleedout::{
    bleedout_system, install_bleedout_rules, is_bleeding_out, is_essential, is_protected,
    spare_from_death, ActorBaseProtectionOverrides, ActorProtection, Bleedout, BleedoutRules,
};
pub use player_control::{
    collect_ghosts, collect_unconscious, AiSuspended, is_ghost, is_unconscious, update_actor_control, ActorControlState, PlayerControlSelection,
    PlayerControlState,
};
pub use quest_stages::{
    install_engine_start_quest, install_quest_alias_readiness_gate, install_start_game_quests,
    quest_alias_readiness_stage_system, quest_startup_system, quest_terminal_stage_system,
    resolve_quest_objective_targets, resolve_quest_targets, QuestAliasReadinessGate,
    QuestDefinitionRegistry, QuestEventRead, QuestEventSubscriberId, QuestFormId, QuestStatus,
    SequencedQuestStageAdvanced, StartGameQuestRegistry, QUEST_EVENT_RETENTION,
};
pub use quest_variables::{
    install_quest_script_variables, quest_variable, QuestScriptObjects, QuestScriptVariableDefaults,
    QuestScriptVariables, ScriptObject,
};
pub use recurring_update::{recurring_update_tick_system, OnUpdateEvent, RecurringUpdate};
pub use registry::{ScriptRegistry, ScriptSpawnFn};
pub use scene::{
    install_scene_quest_aliases, install_scene_records, mark_scene_actor_bindings_dirty,
    quest_alias_diagnostics, quest_alias_refresh_system, refresh_scene_actor_bindings,
    running_quests_binding_entity, running_quest_bound_entities,
    running_quest_bound_entities_cached, scene_playback_system, ActiveSceneAction,
    InventoryItemForms, QuestAliasDiagnostic, QuestAliasInjectedOverlays, QuestAliasInjectionState,
    QuestAliasResolutionState, QuestAliasRuntimeOverlays, RemoteSceneActorStub,
    RunningQuestBoundCache, SceneActionCompletionBatch, SceneActorBindings,
    SceneAliasCandidate, SceneEvent, SceneEventBatch, SceneFragmentInvocation,
    SceneFragmentInvocationBatch, ScenePlaybackState, ScenePlayer, SceneQuestAliasRegistry,
    SceneRegistry, SceneStartRequest, SceneStopRequest,
};
pub use timer::{timer_tick_system, ScriptTimer};
pub use translate::{
    decompile_pex, translate_decompiled_with_providers, translate_pex, translate_pex_detailed,
    translate_pex_detailed_with_providers, translate_script, DecompiledPex,
    CanonicalEvent, PexTranslation, RecognizeCtx, Recognized, ScriptSource,
};
pub use translate::effects::Effect;
pub use trigger::{
    base_form_advance_is_eligible, scene_phase_awaited_stage, trigger_detection_system,
    trigger_object_count, TriggerShape, TriggerVolume,
};
pub use vm_state::{
    two_state_activator_system, ScriptVariables, TwoStateActivator, TwoStateTransition,
    TwoStateTransitionBatch,
};

