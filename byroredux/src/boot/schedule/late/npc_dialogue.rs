//! NPC activation to topic selection, and the access it declares for the spoken-fragment path.
//!
//! Split out of `late.rs` (#6703); the rows, their order and their comments are unchanged.

use byroredux_core::ecs::{Access, Scheduler, Stage};

/// NPC activation to topic selection, and the access it declares for the spoken-fragment path.
pub(super) fn register_npc_dialogue_systems(scheduler: &mut Scheduler) {
    // P4 blocker 1 — NPC activation → topic selection. Runs beside the other
    // Late activation/event readers, before the end-of-Late cleanup drains
    // the ActivateEvent marker. The condition evaluator inside reads the
    // subject's identity and the running-quest tables; the two-pass system
    // installs the selected record into the dialogue registry and stamps
    // the topic on the NPC.
    scheduler.add_exclusive_with_access(
        Stage::Late,
        crate::systems::npc_dialogue::make_npc_dialogue_selection_system(),
        fragment_effect_family_access(bridged_script_getter_reads(Access::new()))
            .reads::<byroredux_scripting::ActivateEvent>()
            .reads::<byroredux_scripting::SceneAliasCandidate>()
            // `IsWeaponSkillType`: the actor's weapon and its skill.
            .reads::<byroredux_core::ecs::components::EquippedWeapon>()
            .reads_resource::<byroredux_scripting::perk_entry_points::WeaponSkillTable>()
            .reads::<byroredux_core::ecs::components::Dead>()
            // #5043 / #5017 — combatant and unconscious NPCs refuse dialogue.
            .reads::<byroredux_scripting::AiCombatState>()
            .reads::<byroredux_scripting::ActorControlState>()
            .reads::<byroredux_core::ecs::components::FormIdComponent>()
            .writes::<crate::systems::npc_dialogue::NpcDialogueTopic>()
            .reads_resource::<crate::systems::PlayerEntity>()
            .reads_resource::<crate::cell_loader::LoadedCellIndex>()
            .reads_resource::<byroredux_scripting::SceneQuestAliasRegistry>()
            .reads_resource::<byroredux_scripting::SceneActorBindings>()
            .reads_resource::<byroredux_scripting::quest_stages::QuestStageState>()
            .writes_resource::<byroredux_scripting::DialogueRegistry>()
            // #5152 — the spoken line's INFO fragment dispatches here: the
            // fragment table feeds the effects, and the executor's guard-free
            // unit journals stage transitions and defers latent tails.
            .reads_resource::<byroredux_scripting::DialogueInfoFragments>()
            // #5307 — the spoken-fragment path WRITES these three:
            // `apply_fragment_guard_free` → `apply_effects` takes
            // `FragmentExecutionQueue` (`try_resource_mut`, effects.rs) for
            // suspended tails; `DeferredFragmentEffects::apply_at_depth`
            // appends `PendingFragmentActivations` and, on a dirty frame,
            // flips `SceneActorBindings.dirty` via
            // `mark_scene_actor_bindings_dirty`. They were declared reads —
            // an under-declaration that would make a parallel-lane
            // promotion of this row look safe when it is not.
            .writes_resource::<byroredux_scripting::FragmentExecutionQueue>()
            .writes_resource::<byroredux_scripting::PendingFragmentActivations>()
            .writes_resource::<byroredux_scripting::SceneActorBindings>()
            // #5307 — the shared `apply_effects` body reaches every Effect
            // variant an authored INFO fragment can carry: enable/lock state
            // writes, cinematic-mode writes, faction-relation writes, and
            // the form/provider/VMAD resolution reads. Surfaced by the
            // `npc_dialogue_selection_declares_everything_…` source scan.
            .writes_resource::<byroredux_scripting::ReferenceEnableState>()
            .writes_resource::<byroredux_scripting::ReferenceLockState>()
            .writes_resource::<byroredux_scripting::ReferenceMotionTypeState>()
            .writes_resource::<byroredux_scripting::CinematicPresentationState>()
            .writes_resource::<byroredux_scripting::FactionRelations>()
            .reads_resource::<byroredux_scripting::LoadOrderIdentity>()
            .reads_resource::<byroredux_scripting::PapyrusProviderRuntime>()
            .reads_resource::<byroredux_scripting::QuestDefinitionRegistry>()
            .reads_resource::<byroredux_scripting::papyrus_demo::PapyrusPlayerEntity>()
            .writes_resource::<byroredux_scripting::quest_stages::QuestStageState>()
            .writes_resource::<byroredux_scripting::quest_stages::QuestObjectiveState>()
            .writes::<byroredux_scripting::quest_stages::QuestStageAdvancedBatch>()
            .writes_resource::<crate::systems::npc_dialogue::DialogueSurfaceState>()
            // #5367 Phase L — the Goodbye close reads the clock, and the
            // spoken Say-Once set is both read (disqualification inside
            // selection) and written (`note_info_spoken` on apply).
            .reads_resource::<byroredux_core::ecs::resources::TotalTime>()
            .writes_resource::<byroredux_scripting::DialogueSpokenInfoForms>()
            .reads_resource::<byroredux_scripting::DialogueRandomState>()
            // #5414 — `apply_selection`'s two cross-file hops. The voice
            // path (`dialogue_voice.rs` `play_line_voice`, called from
            // `apply_selection`) reads the sound-archive provider, the
            // loaded plugin set and (#5393) the form resolver that names
            // the INFO's owning plugin, reads `AudioWorld` for the
            // #5382 device gate then writes it for scheduling, and
            // writes the #5382 `VoiceSoundCache` (voice no longer rides
            // the shared `SoundCache`), and reads the speaker's
            // `GlobalTransform`.
            // The AHEL raise (`story_events.rs` `raise_hello_story_event`,
            // called in Pass 2) writes the `StoryEvent` marker and reads
            // the cell-context resources through `resolve_current_lctn`.
            .reads_resource::<crate::asset_provider::audio::SoundArchiveProvider>()
            .reads_resource::<crate::cell_loader::LoadedPluginSet>()
            .reads_resource::<crate::cell_loader::load_order::GlobalFormIdResolver>()
            .writes_resource::<byroredux_audio::VoiceSoundCache>()
            .writes_resource::<byroredux_audio::AudioWorld>()
            .reads::<byroredux_core::ecs::components::GlobalTransform>()
            .writes::<byroredux_scripting::story_manager::StoryEvent>()
            .reads_resource::<crate::cell_loader::CurrentCellContext>()
            .reads_resource::<crate::cell_loader::CurrentExteriorContext>()
            // #5649 — the INFO speaker/condition gate routes through the
            // scripting crate's `evaluate_function` dispatcher (cross-crate
            // hop, pinned in the `npc_dialogue_selection_declares_everything_…`
            // scan). Storages: the subject-identity and equipment reads
            // (`ActorValues`, `CharacterLevel`, `Background`, `Perks`,
            // `FactionRanks`/`FactionReputation`, `EquipmentSlots`,
            // `Inventory`, `CellRoot`/`CellFormId`, `ScenePlayer`,
            // `ScriptVariables`); resources: `CharacterRuleset`, `Globals`,
            // `SceneRegistry`, `HardcoreMode`, `FormIdPool` reads and the
            // `GetRandomPercent` `StoryManagerRng` write.
            .reads::<byroredux_core::ecs::components::ActorValues>()
            .reads::<byroredux_core::character::CharacterLevel>()
            .reads::<byroredux_core::character::Background>()
            .reads::<byroredux_core::character::Perks>()
            .reads::<byroredux_core::ecs::components::FactionRanks>()
            .reads::<byroredux_core::character::FactionReputation>()
            .reads::<byroredux_core::ecs::components::EquipmentSlots>()
            .reads::<byroredux_core::ecs::components::Inventory>()
            .reads::<byroredux_core::ecs::CellRoot>()
            .reads::<byroredux_core::ecs::CellFormId>()
            .reads::<byroredux_scripting::scene::ScenePlayer>()
            .reads::<byroredux_scripting::vm_state::ScriptVariables>()
            .reads_resource::<byroredux_core::character::CharacterRuleset>()
            .reads_resource::<byroredux_scripting::globals::Globals>()
            .reads_resource::<byroredux_scripting::scene::SceneRegistry>()
            .reads_resource::<byroredux_core::ecs::resources::HardcoreMode>()
            .reads_resource::<byroredux_core::form_id::FormIdPool>()
            .writes_resource::<byroredux_scripting::story_manager::StoryManagerRng>()
            // M47.2 — `GetVMQuestVariable` reads quest-script variables; a
            // spoken line's fragment writes them (`SetQuestVariable`).
            .reads_resource::<byroredux_scripting::QuestScriptVariables>()
            .writes_resource::<byroredux_scripting::QuestScriptVariables>()
            // M47.2 — a spoken line's AddToFaction / RemoveFromFaction edits
            // the speaker's membership and its saved ledger.
            .writes::<byroredux_core::ecs::components::FactionRanks>()
            .writes_resource::<byroredux_scripting::ScriptedFactionChanges>()
            // M47.2 — location conditions (`GetInCurrentLoc`,
            // `GetIsEditorLocAlias`, …) read the LCTN table, the quest
            // location aliases and the player's published location.
            .reads_resource::<byroredux_scripting::LocationTable>()
            .reads_resource::<byroredux_scripting::LocationAliasBindings>()
            .reads_resource::<byroredux_scripting::PlayerLocation>()
            // M47.2 — a spoken line's ForceRefTo / Clear records an alias
            // override (after the quest-state guards drop).
            .writes_resource::<byroredux_scripting::AliasOverrides>()
            // M47.2 — a spoken line's BlockActivation, and the activation
            // gate that reads it.
            .reads_resource::<byroredux_scripting::ReferenceActivationBlocks>()
            .writes_resource::<byroredux_scripting::ReferenceActivationBlocks>()
            // M47.2 — a spoken line's SendStoryEvent queues a script event,
            // its Kill a death request and its RewardPlayerXP a grant.
            .writes_resource::<byroredux_scripting::story_manager::PendingPlayerXp>()
            .writes_resource::<byroredux_scripting::story_manager::PendingScriptStoryEvents>()
            .writes_resource::<byroredux_scripting::story_manager::PendingScriptKills>()
            // M47.2 — a spoken line's AddToMap, and the marker guards.
            .reads_resource::<byroredux_scripting::MapMarkers>()
            .reads_resource::<byroredux_scripting::AuthoredMapMarkers>()
            .writes_resource::<byroredux_scripting::MapMarkers>()
            // M47.2 — a spoken line's SetCleared, and the IsCleared guard.
            .reads_resource::<byroredux_scripting::LocationClearState>()
            .writes_resource::<byroredux_scripting::LocationClearState>()
            // #5778 — the Oblivion quest-level gate functions: GetIsPlayableRace
            // (the RACE Playable set), GetPlayerInSEWorld (the ledger's SE
            // flag), GetIsSex (the bases' ACBS traits) and GetInWorldspace
            // (the cell kind: an exterior root's worldspace, the player's
            // published kind).
            .reads_resource::<byroredux_scripting::obscript_more::PlayableRaces>()
            // #6190 — read by the gate functions, and written by `apply_selection`
            // (`note_talked_to_player`: the speaker has now talked to the player).
            .writes_resource::<byroredux_scripting::obscript_ledger::ObScriptLedger>()
            .reads_resource::<byroredux_scripting::ActorBaseTraits>()
            .reads_resource::<byroredux_scripting::cell_kind::PlayerCellKind>()
            .reads::<byroredux_scripting::cell_kind::ExteriorCell>()
            // #6251 — the getters catalogued from the ObScript VM: GetIsCreature,
            // GetHealthPercentage (the Health stat's id), GetDeadCount (the
            // kill ledger) and GetItemCount's form-list expansion.
            .reads::<byroredux_core::ecs::components::CreatureActor>()
            .reads::<byroredux_core::ecs::components::ActorVitals>()
            .reads_resource::<byroredux_scripting::obscript_state::KillRecords>()
            .reads_resource::<byroredux_scripting::FormLists>()
            .reads_resource::<byroredux_scripting::FormListAdditions>()
            // M47.2 — a spoken line's SetEssential / SetProtected, and the
            // IsEssential / IsBleedingOut guards.
            .reads::<byroredux_scripting::ActorProtection>()
            .reads::<byroredux_scripting::Bleedout>()
            .reads_resource::<byroredux_scripting::ActorBaseProtectionOverrides>()
            .writes_resource::<byroredux_scripting::ActorBaseProtectionOverrides>()
            // M47.2 — the IsDisabled / IsEnabled guards' authored state.
            .reads_resource::<byroredux_scripting::AuthoredDisabledReferences>()
            // M47.2 — quest VMADs for quest-script bodies inlined into lines.
            .reads_resource::<byroredux_scripting::QuestScriptInstances>()
            // M47.2 — a spoken line's reference script-variable writes.
            .writes::<byroredux_scripting::ScriptVariables>()
            // A spoken line's `EquipItem` of a weapon: the request waits for the inventory.
            .writes_resource::<byroredux_scripting::PendingNonArmorEquips>()
            // M47.2 — a spoken line's SetRelationshipRank and its guard.
            .reads_resource::<byroredux_scripting::AuthoredRelationships>()
            .reads_resource::<byroredux_scripting::ScriptedRelationships>()
            .writes_resource::<byroredux_scripting::ScriptedRelationships>()
            // M47.2 — a spoken line's Message.Show / Debug.Notification.
            .reads_resource::<byroredux_scripting::MessageCatalog>()
            .writes_resource::<byroredux_scripting::PendingScriptMessages>()
            // M47.2 — a spoken line's ShowBarterMenu.
            .writes_resource::<byroredux_scripting::menus::PendingBarterMenu>()
            // M47.2 — a spoken line's Add/RemoveKeyword and HasKeyword guard.
            .reads_resource::<byroredux_scripting::keywords::KeywordTable>()
            .reads_resource::<byroredux_scripting::keywords::ScriptedKeywords>()
            .writes_resource::<byroredux_scripting::keywords::ScriptedKeywords>()
            // M47.2 — a spoken line's DeleteWhenAble.
            .writes_resource::<byroredux_scripting::CreatedReferences>()
            // M47.2 — the Papyrus VM's scripts, snapshotted per fragment run.
            .reads_resource::<byroredux_scripting::QuestStageFragments>()
            .reads_resource::<byroredux_scripting::QuestScriptObjects>()
            .writes_resource::<byroredux_scripting::QuestScriptObjects>()
            // M47.2 — a spoken line's update registrations.
            .writes_resource::<byroredux_scripting::ScriptUpdates>()
            // M47.2 — a spoken line's IncrementStat / QueryStat.
            .reads_resource::<byroredux_scripting::MiscStats>()
            .writes_resource::<byroredux_scripting::MiscStats>()
            // M47.2 — a spoken line's AddAchievement.
            .reads_resource::<byroredux_scripting::Achievements>()
            .writes_resource::<byroredux_scripting::Achievements>()
            // M47.2 — a spoken line's run knows what `Self` is for alias scripts.
            .reads_resource::<byroredux_scripting::AliasScriptOwners>()
            // M47.2 — a spoken line's SetLinkedRef / GetLinkedRef.
            .reads_resource::<byroredux_scripting::LinkedRefOverrides>()
            .writes_resource::<byroredux_scripting::LinkedRefOverrides>()
            // M47.3 — a package or topic condition's GetQuestVariable.
            .reads_resource::<byroredux_scripting::obscript_quests::ObScriptQuestLocals>()
            // M47.2 — a spoken line's SetCrimeFaction / GetCrimeFaction (#6701:
            // the subsystem declares its own).
            .with(byroredux_scripting::crime_faction::crime_faction_access)
            // M47.2 — a spoken line's SetBribed / IsIntimidated, ChangeAnimArchetype
            // and SetScanned / AddToMapScanned.
            .reads_resource::<byroredux_scripting::SpeechcraftOutcomes>()
            .writes_resource::<byroredux_scripting::SpeechcraftOutcomes>()
            .reads_resource::<byroredux_scripting::AnimArchetypeOverrides>()
            .writes_resource::<byroredux_scripting::AnimArchetypeOverrides>()
            .reads_resource::<byroredux_scripting::ScannedReferences>()
            .writes_resource::<byroredux_scripting::ScannedReferences>()
            // M47.2 — a spoken line's SetFactionOwner and crime gold natives
            // (#6701: each subsystem declares its own).
            .with(byroredux_scripting::ownership::ownership_access)
            .with(byroredux_scripting::crime_gold::crime_gold_access)
            // M47.2 — a spoken line's FormList.AddForm / GetSize / HasForm.
            .reads_resource::<byroredux_scripting::FormListAdditions>()
            .writes_resource::<byroredux_scripting::FormListAdditions>()
            // M47.2 — a spoken line's SetLocRefType / HasLocRefType.
            .reads_resource::<byroredux_scripting::LocRefTypeOverrides>()
            .writes_resource::<byroredux_scripting::LocRefTypeOverrides>()
            // M47.2 — a spoken line's follower flags.
            .reads_resource::<byroredux_scripting::FollowerState>()
            .writes_resource::<byroredux_scripting::FollowerState>()
            // M47.2 — a spoken line's InputEnableLayer calls.
            .reads_resource::<byroredux_scripting::InputLayers>()
            .writes_resource::<byroredux_scripting::InputLayers>()
            // M47.2 — a spoken line's SetNoFavorAllowed / SetActorCause.
            .writes_resource::<byroredux_scripting::ReferenceFlags>()
            // M47.2 — a spoken line's Sound.Play.
            .writes_resource::<byroredux_scripting::PendingSoundPlays>()
            // M47.2 — a spoken line's Location.GetKeywordData / SetKeywordData.
            .reads_resource::<byroredux_scripting::LocationKeywordData>()
            .writes_resource::<byroredux_scripting::LocationKeywordData>()
            // M47.2 — a spoken line's hit event registrations.
            .writes_resource::<byroredux_scripting::HitRegistrations>()
            // M47.2 — a spoken line's ProcessTrapHit (queued for the next
            // Update flush, #5872) and its shake calls.
            .writes_resource::<byroredux_scripting::PendingTrapHits>()
            .writes::<byroredux_scripting::papyrus_demo::CameraShakeCommand>()
            .writes::<byroredux_scripting::papyrus_demo::ControllerRumbleCommand>()
            // M47.2 — a spoken line's animation event registrations.
            .writes_resource::<byroredux_scripting::AnimationEventRegistrations>()
            // M47.2 — a spoken line's distance registrations.
            .writes_resource::<byroredux_scripting::DistanceWatches>()
            // M47.2 — a spoken line's reference-collection natives.
            .reads_resource::<byroredux_scripting::CollectionAliasKinds>()
            .reads_resource::<byroredux_scripting::AliasCollections>()
            .writes_resource::<byroredux_scripting::AliasCollections>()
            // M47.2 — a spoken line's custom / remote event registrations.
            .reads_resource::<byroredux_scripting::EventRegistrations>()
            .writes_resource::<byroredux_scripting::EventRegistrations>()
            // M47.2 — a spoken line's AdvanceSkill.
            .writes_resource::<byroredux_scripting::PendingSkillAdvances>(),
    );
}

/// #6460 — what the CTDA functions bridged to the ObScript getters (`IsChild`, `GetTalkedToPC`,
/// `GetHasNote`, `IsInList`, `GetLocked`, `GetCurrentTime`, `GetInSameCell`, `GetBaseActorValue`,
/// `GetDestructionStage`, `GetMapMarkerVisible`) read: the same state those getters do. Declared on
/// `npc_dialogue_selection`, which evaluates the INFO conditions. A helper rather than more lines of
/// `register_late_systems`, which is size-pinned (#6359).
///
/// #6558 — and the nine bridged after them: `GetIsCurrentPackage` (`ActivePackages`),
/// `HasMagicEffect` (`ActiveMagicEffects`), `GetSitting` (`Seated`), `IsWeaponOut` (`WeaponDrawn`),
/// `GetDetected` (the installed sight test: detection config, cell lighting, transforms, physics).
/// `IsInInterior`, `IsKiller`, `GetRestrained` and `IsSneaking` read state the row already declares.
///
/// #6545 — and four more: `GetCurrentAIPackage` (the reported package's `PackageRegistry` row, and
/// `AiCombatState` for the combat answer), `IsOwner` (`OwnerOverrides`, `ExplicitOwner`);
/// `GetIsReference` and `GetHitLocation` read nothing the row lacks.
fn bridged_script_getter_reads(access: Access) -> Access {
    access
        .reads_resource::<byroredux_scripting::package::PackageRegistry>()
        .reads::<byroredux_scripting::AiCombatState>()
        .reads_resource::<byroredux_scripting::OwnerOverrides>()
        .reads::<byroredux_scripting::ownership::ExplicitOwner>()
        .reads_resource::<byroredux_scripting::ActivePackages>()
        .reads::<byroredux_scripting::magic_effects::ActiveMagicEffects>()
        .reads::<byroredux_core::ecs::components::Seated>()
        .reads::<byroredux_scripting::obscript_state::WeaponDrawn>()
        .reads_resource::<byroredux_scripting::SightQuery>()
        .reads_resource::<crate::systems::faction_hostility::DetectionConfig>()
        .reads_resource::<crate::components::CellLightingRes>()
        .reads::<byroredux_physics::RapierHandles>()
        .reads_resource::<byroredux_physics::PhysicsWorld>()
        .reads::<byroredux_core::ecs::components::Locked>()
        .reads_resource::<byroredux_scripting::obscript_more::ChildRaces>()
        .reads_resource::<byroredux_scripting::obscript_ledger::ObScriptLedger>()
        .reads_resource::<byroredux_scripting::obscript_state::PipBoyNotes>()
        .reads_resource::<byroredux_scripting::FormLists>()
        .reads_resource::<byroredux_scripting::FormListAdditions>()
        .reads_resource::<byroredux_scripting::fragment::ScriptUpdates>()
        .reads_resource::<byroredux_scripting::map_markers::MapMarkers>()
        .reads_resource::<byroredux_scripting::map_markers::AuthoredMapMarkers>()
}

/// #6685 — the effect families `apply_effect` dispatches to, for
/// `make_npc_dialogue_selection_system`'s row. The callee walk used to stop one
/// hop short of them; the syntax-tree scan, started at
/// `DeferredFragmentEffects::apply_at_depth`, reads their bodies and found
/// these undeclared. The row is exclusive, so nothing mis-scheduled — it is the
/// promotion baseline and the `sys.accesses` view.
///
/// `apply_ai_combat_effect`: combat state and the ambient-engagement marker.
/// `apply_placement_effect`: the moved reference's `Transform` and
/// `EditorLocation`, and the player's `PlayerMoveRequest`.
/// `apply_vehicle_cinematic_effect`: tether, motion-type and cinematic state.
/// `apply_lock_effect`: `Locked`. `apply_scene_effect`: the scene start/stop
/// markers. `apply_player_control_effect`: the player-control flags.
/// `apply_inventory_effect`: the equip-item catalog read.
fn fragment_effect_family_access(access: Access) -> Access {
    access
        .writes::<byroredux_scripting::AiCombatState>()
        .writes::<byroredux_scripting::AmbientEngagement>()
        .writes::<byroredux_core::ecs::Transform>()
        .reads::<byroredux_scripting::EditorLocation>()
        .writes_resource::<byroredux_scripting::PlayerMoveRequest>()
        .writes::<byroredux_scripting::ActorCinematicState>()
        .writes::<byroredux_scripting::HorseTetherState>()
        .writes::<byroredux_scripting::MotionTypeChangeRequest>()
        .writes::<byroredux_core::ecs::components::Locked>()
        .writes::<byroredux_scripting::SceneStartRequest>()
        .writes::<byroredux_scripting::SceneStopRequest>()
        .writes_resource::<byroredux_scripting::PlayerControlState>()
        .reads_resource::<byroredux_scripting::EquipItemCatalog>()
}
