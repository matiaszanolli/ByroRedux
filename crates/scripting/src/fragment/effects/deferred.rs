//! The deferred-effect queue (`DeferredFragmentEffects`), its guard-free apply and the generated-advance poll. (#5468, split from `effects.rs`)

use super::*;

#[derive(Debug)]
pub(super) enum DeferredCinematicPresentationEffect {
    SetSittingRotation(f32),
    RegisterPlayerAnimationEvent {
        event: crate::CinematicAnimationEvent,
        quest: QuestFormId,
        image_space_modifiers: Vec<crate::ImageSpaceModifierApplication>,
    },
    SetInChargen {
        disable_saving: bool,
        disable_waiting: bool,
        show_controls_disabled_message: bool,
    },
    ShowRaceMenu,
    RequestSave {
        auto: bool,
    },
}

/// Resource snapshots and side effects that bracket a fragment's canonical
/// quest-stage/objective guard scope.
///
/// Construct the batch before taking those guards, then apply it after they
/// drop. This prevents nested acquisitions of `QuestDefinitionRegistry` and
/// the presentation/alias-binding resources (#2269, #2539), and — as of
/// #2660 (SCR-D6-NEW11-03) — of `SceneActorBindings` itself: every
/// `resolve_object` alias lookup previously called `world.try_resource::
/// <SceneActorBindings>()` directly, nesting a read acquisition inside the
/// `(QuestStageState, QuestObjectiveState)` scope even after #2539 stopped
/// nesting the *write* side (`mark_scene_actor_bindings_dirty`). Reads now
/// go through the snapshot below instead.
///
/// This does NOT eliminate every nested acquisition inside that scope —
/// `PlayerControlState` (3 writes) and 12 component-storage acquisitions
/// (`Inventory` for `AddItem`, `GlobalTransform`+`Transform` for `MoveTo`,
/// and others across `apply_effect`'s match arms) are direct ECS mutations
/// the effects perform, not alias lookups, and stay nested. See
/// `apply_effect`'s doc for the current complete list and the exclusive-
/// scheduling invariant that makes it safe today.
/// One queued edit to [`crate::ReferenceLockState`]. Mirrors the three
/// shapes the live `Locked` component can take after a scripted effect,
/// so the ledger and the component cannot drift (#4136).
#[derive(Debug, Clone, Copy)]
pub(super) enum DeferredLockChange {
    Locked {
        lock_level: u8,
        key_form_id: Option<u32>,
    },
    Unlocked,
}

#[derive(Debug)]
pub struct DeferredFragmentEffects {
    pub(super) quest_definitions: Option<crate::QuestDefinitionRegistry>,
    /// #2660 (SCR-D6-NEW11-03) — snapshot of `SceneActorBindings`, cloned
    /// before the quest-state guards so `resolve_object` can resolve
    /// alias-bound `ObjectRef`s without a nested resource acquisition.
    /// `SceneActorBindings::resolve` only reads; nothing inside the guard
    /// scope can invalidate this snapshot mid-cascade (the table is only
    /// ever rebuilt by a separate, later-scheduled system after this one's
    /// `scene_actor_bindings_dirty` flag below is applied), so a snapshot
    /// taken once at the top of the exclusive system is behaviorally
    /// identical to a live read throughout.
    pub(crate) scene_actor_bindings: crate::scene::SceneActorBindings,
    /// M47.2 — the speaker of the dialogue line whose fragment is running
    /// (`ActorRef::Speaker`). `None` for every quest / scene fragment.
    pub(crate) speaker: Option<EntityId>,
    pub(super) cinematic_presentation: Vec<DeferredCinematicPresentationEffect>,
    pub(super) scene_actor_bindings_dirty: bool,
    /// `(target, activator)` pairs from `Effect::Activate`, handed to
    /// [`PendingFragmentActivations`] so they are delivered at the head of
    /// the *next* frame (#2654) — see that resource's docs.
    pub(super) activations: Vec<(EntityId, EntityId)>,
    /// The `Activate(…, true)` ones (default processing only).
    pub(super) native_activations: Vec<(EntityId, EntityId)>,
    pub(in crate::fragment) reference_enable_changes: Vec<(u32, bool)>,
    /// #4136 — scripted lock changes, FormID-keyed for the same reason
    /// `reference_enable_changes` above is: the state has to outlive the
    /// reference's cell, and a component cannot survive its own entity's
    /// despawn on cell unload.
    pub(super) reference_lock_changes: Vec<(u32, DeferredLockChange)>,
    /// `Effect::SetEnemy` / `Effect::SetAlly` calls — `(faction,
    /// other_faction, faction's reaction, other's reaction)` in global
    /// FormIDs — applied to `FactionRelations` after the quest-state guards
    /// drop. That resource follows the same nested-acquisition caution as
    /// `CinematicPresentationState` above (#2269), not the "direct component
    /// storage, stays nested" exception.
    pub(super) faction_relations: Vec<(
        u32,
        u32,
        byroredux_sdk::relationships::CombatReaction,
        byroredux_sdk::relationships::CombatReaction,
    )>,
    /// M47.2 — `Effect::ChangeFaction` calls, applied to `FactionRanks` and
    /// recorded in [`crate::ScriptedFactionChanges`] after the guards drop.
    pub(super) faction_changes: Vec<(EntityId, u32, crate::FactionChange)>,
    /// M47.2 — `Effect::ForceAlias` overrides, recorded in
    /// [`crate::AliasOverrides`] after the quest-state guards drop (the alias
    /// refresh takes `SceneActorBindings` before `QuestStageState`).
    pub(super) alias_overrides: Vec<(QuestFormId, i32, crate::AliasOverride)>,
    /// M47.2 — `Effect::SendStoryEvent` events, queued for the Story Manager
    /// after the guards drop.
    pub(super) story_events: Vec<crate::story_manager::StoryEvent>,
    /// M47.2 — `Effect::Kill` requests, queued in
    /// [`crate::story_manager::PendingScriptKills`] after the guards drop.
    pub(super) kills: Vec<(EntityId, Option<EntityId>, bool)>,
    /// M47.2 — `Effect::RewardPlayerXp` grants, queued in
    /// [`crate::story_manager::PendingPlayerXp`] after the guards drop.
    pub(super) player_xp: Vec<(u32, bool)>,
    /// M47.2 — `Effect::IncrementStat`s, applied to [`crate::MiscStats`]
    /// after the guards drop.
    pub(super) misc_stats: Vec<(String, i32)>,
    /// M47.2 — `Effect::AddAchievement` IDs, applied to
    /// [`crate::Achievements`] after the guards drop.
    pub(super) achievements: Vec<i32>,
    /// M47.2 — `Effect::AdvanceSkill`s, queued on
    /// [`crate::PendingSkillAdvances`] after the guards drop.
    pub(super) skill_advances: Vec<(String, f32)>,
    /// M47.2 — `Effect::SetLinkedRef`s: (source, keyword, target), applied to
    /// [`crate::LinkedRefOverrides`] after the guards drop.
    pub(super) linked_ref_changes: Vec<(u32, u32, u32)>,
    /// M47.2 — `Effect::SetLocRefType`s: (reference, ref type or 0), applied to
    /// [`crate::LocRefTypeOverrides`] after the guards drop.
    pub(super) loc_ref_type_changes: Vec<(u32, u32)>,
    /// M47.2 — `Effect::SetCrimeFaction`s: (actor reference, faction or 0),
    /// applied to [`crate::CrimeFactionOverrides`] after the guards drop.
    pub(crate) crime_faction_changes: Vec<(u32, u32)>,
    /// M47.2 — `SetBribed` / `SetIntimidated` calls: (actor reference, outcome,
    /// flagged), applied to [`crate::SpeechcraftOutcomes`] after the guards drop.
    pub(crate) speechcraft_changes: Vec<(u32, crate::SpeechcraftOutcome, bool)>,
    /// M47.2 — `ChangeAnimArchetype` calls: (actor reference, keyword or 0),
    /// applied to [`crate::AnimArchetypeOverrides`] after the guards drop.
    pub(crate) anim_archetype_changes: Vec<(u32, u32)>,
    /// M47.2 — form list edits a run made, applied to
    /// [`crate::FormListAdditions`] after the guards drop (and visible to the
    /// run's own later reads).
    pub(crate) form_list_changes: Vec<crate::FormListChange>,
    /// M47.2 — the handle a run draws `PlaceAtMe` FormIDs through, taken
    /// before the quest-state guards so the VM never locks
    /// [`crate::CreatedReferences`].
    pub(in crate::fragment) scripted_ids: Option<crate::ScriptedIds>,
    /// M47.2 — the owner ids of the alias script instances and what each is
    /// attached to, snapshotted so a run knows what `Self` is.
    pub(in crate::fragment) alias_owners: Arc<HashMap<u32, (u32, i16)>>,
    /// M47.2 — `PlaceAtMe` creations: (FormID, base, placed at,
    /// delete when able), recorded on [`crate::CreatedReferences`] after the
    /// guards drop.
    pub(in crate::fragment) created_references: Vec<(u32, u32, u32, bool)>,
    /// M47.2 — follower-flag writes: (actor FormID, teammate (with can-do-
    /// favor) or doing-favor), applied to [`crate::FollowerState`] after the
    /// guards drop.
    pub(super) follower_changes: Vec<(u32, FollowerChange)>,
    /// M47.2 — distance registrations a Papyrus VM run asked for: the owner,
    /// the script, the change.
    pub(in crate::fragment) distance_requests: Vec<(u32, String, super::DistanceRequest)>,
    /// M47.2 — hit event registration changes, applied to
    /// [`super::HitRegistrations`] after the guards drop.
    pub(in crate::fragment) hit_requests: Vec<(u32, String, super::HitRequest)>,
    /// `RegisterForAnimationEvent` changes, applied to
    /// [`super::AnimationEventRegistrations`] after the guards drop.
    pub(in crate::fragment) animation_event_requests: Vec<(u32, String, super::AnimationEventRequest)>,
    /// M47.2 — `Sound.Play` calls, queued on [`crate::PendingSoundPlays`] after
    /// the guards drop.
    pub(in crate::fragment) sound_plays: Vec<crate::SoundPlay>,
    /// `Game.ShakeCamera` / `Game.ShakeController` calls, placed as the player's
    /// transient command markers once the guards drop.
    pub(in crate::fragment) camera_shakes: Vec<crate::papyrus_demo::CameraShakeCommand>,
    pub(in crate::fragment) controller_shakes: Vec<crate::papyrus_demo::ControllerRumbleCommand>,
    /// `ProcessTrapHit` calls: `(target, trap, damage)`, queued on
    /// [`crate::PendingTrapHits`] once the guards drop (landed as the
    /// target's `HitEvent` by `pending_trap_hit_flush_system`, #5872).
    pub(in crate::fragment) trap_hits: Vec<(byroredux_core::ecs::EntityId, byroredux_core::ecs::EntityId, f32)>,
    /// M47.2 — `SetNoFavorAllowed` / `SetActorCause` calls, applied to
    /// [`crate::ReferenceFlags`] after the guards drop.
    pub(in crate::fragment) reference_flag_changes: Vec<crate::ReferenceFlagChange>,
    /// M47.2 — `SetActorOwner` / `SetFactionOwner` calls: (reference, owner or
    /// 0), applied to [`crate::OwnerOverrides`] after the guards drop.
    pub(crate) owner_changes: Vec<(u32, u32)>,
    /// M47.2 — faction crime gold edits, applied to [`crate::FactionCrimeGold`]
    /// after the guards drop (and visible to the run's own later reads).
    pub(crate) crime_gold_changes: Vec<crate::CrimeGoldChange>,
    /// M47.2 — `Effect::SetLocationKeywordData`s: (location, keyword, data),
    /// applied to [`crate::LocationKeywordData`] after the guards drop.
    pub(super) location_keyword_changes: Vec<(u32, u32, f32)>,
    /// M47.2 — reference-collection members a Papyrus VM run wrote back,
    /// applied to [`crate::AliasCollections`] after the guards drop.
    pub(in crate::fragment) alias_collections: Vec<(crate::alias_collections::CollectionKey, Vec<u32>)>,
    /// M47.2 — custom / remote event registration changes a Papyrus VM run
    /// asked for: the owner, the script, the change.
    pub(in crate::fragment) event_requests: Vec<(u32, String, super::EventRequest)>,
    /// Which collection aliases the quests leave to their scripts, taken
    /// before any quest-state lock (see [`crate::CollectionAliasKinds`]).
    pub(in crate::fragment) collection_kinds: std::sync::Arc<HashMap<crate::alias_collections::CollectionKey, bool>>,
    /// M47.2 — `Effect::SetLocal` values for this fragment run.
    pub(super) locals: HashMap<String, f32>,
    /// M47.2 — `Effect::SetReferenceVariable` writes, applied to the
    /// references' `ScriptVariables` after the guards drop.
    pub(super) reference_variables: Vec<(EntityId, String, f32)>,
    /// M47.2 — messages to show, formatted after the guards drop (text
    /// replacement reads the alias bindings).
    pub(super) messages: Vec<PendingMessage>,
    /// M47.2 — the vendor an `Effect::ShowBarterMenu` asked to trade with.
    pub(super) barter: Option<EntityId>,
    /// M47.2 — `Effect::DeleteWhenAble` references, flagged in
    /// [`crate::CreatedReferences`] after the guards drop.
    pub(super) deletions: Vec<u32>,
    /// M47.2 — `Effect::ChangeKeyword`s, applied to
    /// [`crate::keywords::ScriptedKeywords`] after the guards drop.
    pub(super) keyword_changes: Vec<(EntityId, u32, crate::keywords::KeywordChange)>,
    /// M47.2 — `Effect::AddToMap` additions, recorded in
    /// [`crate::MapMarkers`] after the guards drop.
    pub(crate) map_markers: Vec<(u32, bool)>,
    /// `SetMarkerVisibleOnStarMap(visible)` calls: (marker, visible).
    pub(crate) star_map_changes: Vec<(u32, bool)>,
    /// M47.2 — `SetScanned` / `AddToMapScanned` flags: (reference, scanned),
    /// applied to [`crate::ScannedReferences`] after the guards drop.
    pub(crate) scanned_changes: Vec<(u32, bool)>,
    /// M47.2 — `Effect::SetLocationCleared` changes, recorded in
    /// [`crate::LocationClearState`] after the guards drop.
    pub(super) location_clears: Vec<(u32, bool)>,
    /// M47.2 — `Effect::SetBaseProtection` writes `(base, protected,
    /// value)`, recorded in [`crate::ActorBaseProtectionOverrides`] after
    /// the guards drop.
    pub(super) base_protection: Vec<(u32, bool, bool)>,
    /// M47.2 — `Effect::SetRelationshipRank` writes by base pair, recorded in
    /// [`crate::ScriptedRelationships`] after the guards drop.
    pub(crate) relationship_changes: Vec<((u32, u32), i8)>,
    /// M47.2 — `Effect::BlockActivation` changes, recorded in
    /// [`super::ReferenceActivationBlocks`] after the guards drop.
    pub(super) activation_blocks: Vec<(u32, bool)>,
    /// M47.2 — the scenes playing when the fragment started, for
    /// `FragmentGuard::ScenePlaying`: snapshotted before the quest-state
    /// guards (scene playback takes `ScenePlayer` before `QuestStageState`).
    pub(super) running_scenes: std::collections::HashSet<u32>,
    /// M47.2 — every quest's VMAD, for `Effect::QuestScope`: snapshotted (an
    /// `Arc` clone) before the quest-state guards.
    pub(in crate::fragment) quest_scripts: std::sync::Arc<HashMap<QuestFormId, ScriptInstanceData>>,
    /// M47.2 — the Papyrus VM's scripts ([`QuestStageFragments::vm_scripts`]),
    /// snapshotted with the quest VMADs.
    pub(in crate::fragment) vm_scripts: std::sync::Arc<HashMap<String, std::sync::Arc<byroredux_papyrus::ast::Script>>>,
    /// M47.2 — `Effect::SetQuestVariable` writes, applied to
    /// [`crate::QuestScriptVariables`] after the quest-state guards drop.
    pub(in crate::fragment) quest_variables: Vec<(QuestFormId, byroredux_plugin::esm::records::condition::ConditionStringId, f32)>,
    /// M47.2 — the Papyrus VM's object script-variable writes, applied to
    /// [`crate::QuestScriptObjects`] after the quest-state guards drop.
    pub(in crate::fragment) quest_objects: Vec<(
        QuestFormId,
        byroredux_plugin::esm::records::condition::ConditionStringId,
        crate::ScriptObject,
    )>,
    /// M47.2 — update registrations Papyrus VM runs asked for: the owner,
    /// the script, the change.
    pub(in crate::fragment) script_updates: Vec<(u32, String, super::UpdateRequest)>,
    pub(super) provider_steps: Vec<DeferredProviderFragmentStep>,
}

#[derive(Debug)]
pub(super) struct DeferredProviderFragmentStep {
    pub(super) call: crate::translate::effects::FragmentProviderCall,
    pub(super) context: QuestFormId,
    pub(super) vmad: Option<ScriptInstanceData>,
    pub(super) tail: Vec<Effect>,
}

impl DeferredFragmentEffects {
    /// Snapshot fragment resources before acquiring the quest-state guards.
    ///
    /// Production callers take this snapshot before acquiring quest-state
    /// guards. `QuestDefinitionRegistry::clone()` is an O(1) `Arc`
    /// refcount bump (#2659 / SCR-D6-NEW11-02), not a deep copy — that's
    /// what keeps each fragment snapshot cheap on every load-order size.
    /// `SceneActorBindings::clone()` (#2660) is a real `HashMap` clone, but
    /// the table only holds bound aliases for currently-active quests, not
    /// the whole load order — negligible next to the per-frame ECS work
    /// this system already does.
    pub fn new(world: &World) -> Self {
        Self {
            quest_definitions: world
                .try_resource::<crate::QuestDefinitionRegistry>()
                .map(|definitions| definitions.clone()),
            scene_actor_bindings: world
                .try_resource::<crate::scene::SceneActorBindings>()
                .map(|bindings| bindings.clone())
                .unwrap_or_default(),
            speaker: None,
            cinematic_presentation: Vec::new(),
            scene_actor_bindings_dirty: false,
            activations: Vec::new(),
            native_activations: Vec::new(),
            reference_enable_changes: Vec::new(),
            reference_lock_changes: Vec::new(),
            faction_relations: Vec::new(),
            quest_variables: Vec::new(),
            quest_objects: Vec::new(),
            script_updates: Vec::new(),
            faction_changes: Vec::new(),
            alias_overrides: Vec::new(),
            activation_blocks: Vec::new(),
            map_markers: Vec::new(),
            star_map_changes: Vec::new(),
            location_clears: Vec::new(),
            base_protection: Vec::new(),
            relationship_changes: Vec::new(),
            story_events: Vec::new(),
            kills: Vec::new(),
            player_xp: Vec::new(),
            misc_stats: Vec::new(),
            achievements: Vec::new(),
            skill_advances: Vec::new(),
            linked_ref_changes: Vec::new(),
            loc_ref_type_changes: Vec::new(),
            crime_faction_changes: Vec::new(),
            speechcraft_changes: Vec::new(),
            anim_archetype_changes: Vec::new(),
            scanned_changes: Vec::new(),
            form_list_changes: Vec::new(),
            scripted_ids: world
                .try_resource::<crate::CreatedReferences>()
                .map(|created| created.scripted_ids()),
            created_references: Vec::new(),
            alias_owners: world
                .try_resource::<crate::AliasScriptOwners>()
                .map(|owners| owners.table())
                .unwrap_or_default(),
            follower_changes: Vec::new(),
            distance_requests: Vec::new(),
            hit_requests: Vec::new(),
            animation_event_requests: Vec::new(),
            sound_plays: Vec::new(),
            camera_shakes: Vec::new(),
            controller_shakes: Vec::new(),
            trap_hits: Vec::new(),
            reference_flag_changes: Vec::new(),
            owner_changes: Vec::new(),
            crime_gold_changes: Vec::new(),
            location_keyword_changes: Vec::new(),
            alias_collections: Vec::new(),
            event_requests: Vec::new(),
            collection_kinds: world
                .try_resource::<crate::CollectionAliasKinds>()
                .map(|kinds| std::sync::Arc::clone(&kinds.0))
                .unwrap_or_default(),
            locals: HashMap::new(),
            reference_variables: Vec::new(),
            messages: Vec::new(),
            barter: None,
            deletions: Vec::new(),
            keyword_changes: Vec::new(),
            quest_scripts: world
                .try_resource::<super::QuestScriptInstances>()
                .map(|instances| instances.0.clone())
                .unwrap_or_default(),
            vm_scripts: world
                .try_resource::<super::QuestStageFragments>()
                .map(|fragments| fragments.vm_scripts())
                .unwrap_or_default(),
            running_scenes: world
                .query::<crate::scene::ScenePlayer>()
                .map(|players| {
                    players
                        .iter()
                        .filter(|(_, player)| player.is_running())
                        .map(|(_, player)| player.scene_form_id)
                        .collect()
                })
                .unwrap_or_default(),
            provider_steps: Vec::new(),
        }
    }

    /// Apply queued mutations after releasing the `QuestStageState` /
    /// `QuestObjectiveState` guards passed to [`apply_effects`].
    pub fn apply(self, world: &World) -> Vec<QuestStageAdvanced> {
        self.apply_at_depth(world, 0)
    }

    /// Apply one cascade level of the queued effects (#5803): the flush
    /// methods below run in the original queue order — that order is
    /// behaviour — followed by the provider-step tail, which recurses at
    /// `depth + 1` and consumes `self`.
    fn apply_at_depth(mut self, world: &World, depth: usize) -> Vec<QuestStageAdvanced> {
        // #3946 — the barrier cap used to be tested here, and returned
        // before the four non-provider flushes below. That was partial
        // application: the `deferred` arriving at this depth was filled by
        // a tail whose `stages`/`objectives` writes had *already been
        // committed* under the caller's guards, so bailing here advanced
        // the quest while silently discarding the scene-binding,
        // activation, reference-enable and cinematic effects that were
        // queued alongside it. The cap now gates the recursion at its
        // source, below, the way `MAX_CASCADE` gates the cascade loop:
        // by declining to start work it cannot finish. Depth can therefore
        // never reach the cap here.
        debug_assert!(
            depth < MAX_PROVIDER_FRAGMENT_BARRIERS,
            "recursion is gated before descending; depth {depth} should be unreachable"
        );
        self.flush_messages(world);
        self.flush_deletions(world);
        self.flush_barter(world);
        self.flush_keyword_changes(world);
        self.flush_reference_variables(world);
        self.flush_event_requests(world);
        self.flush_alias_collections(world);
        self.flush_location_keyword_changes(world);
        self.flush_reference_flag_changes(world);
        self.flush_owner_changes(world);
        self.flush_crime_gold_changes(world);
        self.flush_shakes(world);
        self.flush_trap_hits(world);
        self.flush_sound_plays(world);
        self.flush_hit_requests(world);
        self.flush_animation_event_requests(world);
        self.flush_distance_requests(world);
        self.flush_follower_changes(world);
        self.flush_created_references(world);
        self.flush_linked_ref_changes(world);
        self.flush_crime_faction_changes(world);
        self.flush_speechcraft_changes(world);
        self.flush_scanned_changes(world);
        self.flush_anim_archetype_changes(world);
        self.flush_form_list_changes(world);
        self.flush_loc_ref_type_changes(world);
        self.flush_skill_advances(world);
        self.flush_achievements(world);
        self.flush_misc_stats(world);
        self.flush_player_xp(world);
        self.flush_kills(world);
        self.flush_story_events(world);
        self.flush_star_map_changes(world);
        self.flush_map_markers(world);
        self.flush_relationship_changes(world);
        self.flush_base_protection(world);
        self.flush_location_clears(world);
        self.flush_activation_blocks(world);
        self.flush_alias_overrides(world);
        self.flush_scene_actor_bindings_dirty(world);
        self.flush_activations(world);
        self.flush_reference_enable_changes(world);
        self.flush_reference_lock_changes(world);
        self.flush_cinematic_presentation(world);
        self.flush_faction_changes(world);
        self.flush_quest_variables(world);
        self.flush_quest_objects(world);
        self.flush_script_updates(world);
        self.flush_faction_relations(world);

        if self.provider_steps.is_empty() {
            return Vec::new();
        }
        let Some(callback) = world
            .try_resource::<crate::PapyrusProviderRuntime>()
            .and_then(|runtime| runtime.callback())
        else {
            log::warn!("deferred fragment provider calls dropped: provider host is unavailable");
            return Vec::new();
        };

        // Loop-invariant in `depth`, so it is computed once and warns once
        // rather than per remaining step.
        let at_barrier_limit = depth + 1 >= MAX_PROVIDER_FRAGMENT_BARRIERS;
        if at_barrier_limit {
            log::warn!(
                "fragment provider continuation reached {MAX_PROVIDER_FRAGMENT_BARRIERS} \
                 barriers; declining deeper tails (their effects are not applied)"
            );
        }

        let mut advances = Vec::new();
        for step in std::mem::take(&mut self.provider_steps) {
            if let Err(error) = callback(
                step.call.principal.as_ref(),
                &step.call.route,
                &step.call.arguments,
            ) {
                log::warn!("deferred fragment provider call aborted: {error}");
                continue;
            }
            if step.tail.is_empty() {
                continue;
            }
            if at_barrier_limit {
                // Skip the tail *whole*. Running `apply_effects` first and
                // bailing afterwards is what produced the partial state
                // this fix removes: the provider call above is the barrier
                // itself and has already happened, but nothing downstream
                // of it is committed.
                continue;
            }
            let mut deferred = Self::new(world);
            let resumed = {
                let (mut stages, mut objectives) =
                    world.resource_2_mut::<QuestStageState, QuestObjectiveState>();
                apply_effects(
                    &step.tail,
                    step.context,
                    step.vmad.as_ref(),
                    world,
                    &mut stages,
                    &mut objectives,
                    &mut deferred,
                )
            };
            advances.extend(resumed);
            advances.extend(deferred.apply_at_depth(world, depth + 1));
        }
        advances
    }

    fn flush_messages(&mut self, world: &World) {
        for message in std::mem::take(&mut self.messages) {
            match message {
                PendingMessage::Show(form_id, args) => {
                    if !crate::messages::show_message(world, form_id, &args) {
                        log::debug!("fragment Show skipped: no message {form_id:08X}");
                    }
                }
                PendingMessage::Notify(text) => crate::messages::notify(world, text),
                PendingMessage::ItemAdded(item_form_id, count) => {
                    crate::messages::notify(world, crate::messages::item_added_text(world, item_form_id, count));
                }
            }
        }
    }

    fn flush_deletions(&mut self, world: &World) {
        for form_id in std::mem::take(&mut self.deletions) {
            let created = world
                .try_resource_mut::<crate::CreatedReferences>()
                .is_some_and(|mut created| created.delete_when_able(form_id));
            if !created {
                // CK *Delete*: "Only references created in game can actually
                // be deleted."
                log::debug!("fragment DeleteWhenAble: {form_id:08X} is not a created reference");
            }
        }
    }

    fn flush_barter(&mut self, world: &World) {
        if let Some(vendor) = self.barter.take() {
            crate::menus::request_barter(world, vendor);
        }
    }

    fn flush_keyword_changes(&mut self, world: &World) {
        for (object, keyword, change) in std::mem::take(&mut self.keyword_changes) {
            if !crate::keywords::change_keyword(world, object, keyword, change) {
                log::debug!("fragment {change:?}Keyword skipped: the reference has no FormID");
            }
        }
    }

    fn flush_reference_variables(&mut self, world: &World) {
        if !self.reference_variables.is_empty() {
            if let Some(mut variables) = world.query_mut::<crate::vm_state::ScriptVariables>() {
                for (entity, variable, value) in std::mem::take(&mut self.reference_variables) {
                    if variables.get_mut(entity).is_none() {
                        variables.insert(entity, crate::vm_state::ScriptVariables::default());
                    }
                    variables
                        .get_mut(entity)
                        .expect("present or inserted")
                        .set_by_name(&variable, value);
                }
            }
        }
    }

    fn flush_event_requests(&mut self, world: &World) {
        if !self.event_requests.is_empty() {
            if let Some(mut registrations) = world.try_resource_mut::<super::EventRegistrations>() {
                for (owner, script, request) in self.event_requests.drain(..) {
                    registrations.request(owner, &script, &request);
                }
            }
            self.event_requests.clear();
        }
    }

    fn flush_alias_collections(&mut self, world: &World) {
        if !self.alias_collections.is_empty() {
            if let Some(mut collections) = world.try_resource_mut::<crate::AliasCollections>() {
                for (key, members) in self.alias_collections.drain(..) {
                    collections.set(key, members);
                }
            }
            self.alias_collections.clear();
        }
    }

    fn flush_location_keyword_changes(&mut self, world: &World) {
        if !self.location_keyword_changes.is_empty() {
            if let Some(mut data) = world.try_resource_mut::<crate::LocationKeywordData>() {
                for (location, keyword, value) in self.location_keyword_changes.drain(..) {
                    data.set(location, keyword, value);
                }
            }
            self.location_keyword_changes.clear();
        }
    }

    fn flush_reference_flag_changes(&mut self, world: &World) {
        if !self.reference_flag_changes.is_empty() {
            if let Some(mut flags) = world.try_resource_mut::<crate::ReferenceFlags>() {
                for change in self.reference_flag_changes.drain(..) {
                    flags.apply(change);
                }
            }
            self.reference_flag_changes.clear();
        }
    }

    fn flush_owner_changes(&mut self, world: &World) {
        if !self.owner_changes.is_empty() {
            if let Some(mut owners) = world.try_resource_mut::<crate::OwnerOverrides>() {
                for (reference, owner) in self.owner_changes.drain(..) {
                    owners.set(reference, owner);
                }
            }
            self.owner_changes.clear();
        }
    }

    fn flush_crime_gold_changes(&mut self, world: &World) {
        if !self.crime_gold_changes.is_empty() {
            if let Some(mut gold) = world.try_resource_mut::<crate::FactionCrimeGold>() {
                for change in self.crime_gold_changes.drain(..) {
                    gold.apply(change);
                }
            }
            self.crime_gold_changes.clear();
        }
    }

    fn flush_shakes(&mut self, world: &World) {
        if !self.camera_shakes.is_empty() || !self.controller_shakes.is_empty() {
            let player = crate::papyrus_demo::PapyrusPlayerEntity::get(world);
            if let Some(player) = player {
                if let Some(mut shakes) = world.query_mut::<crate::papyrus_demo::CameraShakeCommand>() {
                    if let Some(shake) = self.camera_shakes.last() {
                        shakes.insert(player, *shake);
                    }
                }
                if let Some(mut shakes) = world.query_mut::<crate::papyrus_demo::ControllerRumbleCommand>() {
                    if let Some(shake) = self.controller_shakes.last() {
                        shakes.insert(player, *shake);
                    }
                }
            }
            self.camera_shakes.clear();
            self.controller_shakes.clear();
        }
    }

    fn flush_trap_hits(&mut self, world: &World) {
        if !self.trap_hits.is_empty() {
            if let Some(mut pending) = world.try_resource_mut::<crate::PendingTrapHits>() {
                pending.0.extend(
                    self.trap_hits
                        .drain(..)
                        .map(|(target, trap, damage)| crate::TrapHit { target, trap, damage }),
                );
            }
            self.trap_hits.clear();
        }
    }

    fn flush_sound_plays(&mut self, world: &World) {
        if !self.sound_plays.is_empty() {
            if let Some(mut pending) = world.try_resource_mut::<crate::PendingSoundPlays>() {
                pending.0.append(&mut self.sound_plays);
            }
            self.sound_plays.clear();
        }
    }

    fn flush_hit_requests(&mut self, world: &World) {
        if !self.hit_requests.is_empty() {
            if let Some(mut registrations) = world.try_resource_mut::<super::HitRegistrations>() {
                for (owner, script, request) in self.hit_requests.drain(..) {
                    registrations.request(owner, &script, &request);
                }
            }
            self.hit_requests.clear();
        }
    }

    fn flush_animation_event_requests(&mut self, world: &World) {
        if !self.animation_event_requests.is_empty() {
            if let Some(mut registrations) = world.try_resource_mut::<super::AnimationEventRegistrations>() {
                for (owner, script, request) in self.animation_event_requests.drain(..) {
                    registrations.request(owner, &script, &request);
                }
            }
            self.animation_event_requests.clear();
        }
    }

    fn flush_distance_requests(&mut self, world: &World) {
        if !self.distance_requests.is_empty() {
            if let Some(mut watches) = world.try_resource_mut::<super::DistanceWatches>() {
                for (owner, script, request) in self.distance_requests.drain(..) {
                    watches.request(owner, &script, request);
                }
            }
            self.distance_requests.clear();
        }
    }

    fn flush_follower_changes(&mut self, world: &World) {
        if !self.follower_changes.is_empty() {
            if let Some(mut state) = world.try_resource_mut::<crate::FollowerState>() {
                for (actor, change) in self.follower_changes.drain(..) {
                    match change {
                        FollowerChange::Teammate { teammate, can_do_favor } => {
                            state.set_teammate(actor, teammate, can_do_favor)
                        }
                        FollowerChange::DoingFavor(doing) => state.set_doing_favor(actor, doing),
                    }
                }
            }
            self.follower_changes.clear();
        }
    }

    fn flush_created_references(&mut self, world: &World) {
        if !self.created_references.is_empty() {
            match world.try_resource_mut::<crate::CreatedReferences>() {
                Some(mut created) => {
                    for (form_id, base, at, delete_when_able) in self.created_references.drain(..) {
                        created.create_scripted(form_id, base, at, delete_when_able);
                    }
                }
                None => log::debug!("PlaceAtMe dropped: CreatedReferences is unavailable"),
            }
        }
    }

    fn flush_linked_ref_changes(&mut self, world: &World) {
        if !self.linked_ref_changes.is_empty() {
            if let Some(mut links) = world.try_resource_mut::<crate::LinkedRefOverrides>() {
                for (source, keyword, target) in self.linked_ref_changes.drain(..) {
                    links.set(source, keyword, target);
                }
            }
            self.linked_ref_changes.clear();
        }
    }

    fn flush_crime_faction_changes(&mut self, world: &World) {
        if !self.crime_faction_changes.is_empty() {
            if let Some(mut overrides) = world.try_resource_mut::<crate::CrimeFactionOverrides>() {
                for (actor, faction) in self.crime_faction_changes.drain(..) {
                    overrides.set(actor, faction);
                }
            }
            self.crime_faction_changes.clear();
        }
    }

    fn flush_speechcraft_changes(&mut self, world: &World) {
        if !self.speechcraft_changes.is_empty() {
            if let Some(mut outcomes) = world.try_resource_mut::<crate::SpeechcraftOutcomes>() {
                for (actor, outcome, flagged) in self.speechcraft_changes.drain(..) {
                    outcomes.set(actor, outcome, flagged);
                }
            }
            self.speechcraft_changes.clear();
        }
    }

    fn flush_scanned_changes(&mut self, world: &World) {
        if !self.scanned_changes.is_empty() {
            if let Some(mut scanned) = world.try_resource_mut::<crate::ScannedReferences>() {
                for (reference, flagged) in self.scanned_changes.drain(..) {
                    scanned.set(reference, flagged);
                }
            }
            self.scanned_changes.clear();
        }
    }

    fn flush_anim_archetype_changes(&mut self, world: &World) {
        if !self.anim_archetype_changes.is_empty() {
            if let Some(mut overrides) = world.try_resource_mut::<crate::AnimArchetypeOverrides>() {
                for (actor, keyword) in self.anim_archetype_changes.drain(..) {
                    overrides.set(actor, keyword);
                }
            }
            self.anim_archetype_changes.clear();
        }
    }

    fn flush_form_list_changes(&mut self, world: &World) {
        if !self.form_list_changes.is_empty() {
            if let Some(mut additions) = world.try_resource_mut::<crate::FormListAdditions>() {
                for change in self.form_list_changes.drain(..) {
                    additions.apply(change);
                }
            }
            self.form_list_changes.clear();
        }
    }

    fn flush_loc_ref_type_changes(&mut self, world: &World) {
        if !self.loc_ref_type_changes.is_empty() {
            if let Some(mut types) = world.try_resource_mut::<crate::LocRefTypeOverrides>() {
                for (reference, ref_type) in self.loc_ref_type_changes.drain(..) {
                    types.set(reference, ref_type);
                }
            }
            self.loc_ref_type_changes.clear();
            // An alias may now fill by the new type.
            crate::scene::mark_scene_actor_bindings_dirty(world);
        }
    }

    fn flush_skill_advances(&mut self, world: &World) {
        if !self.skill_advances.is_empty() {
            if let Some(mut pending) = world.try_resource_mut::<crate::PendingSkillAdvances>() {
                pending.0.append(&mut self.skill_advances);
            }
            self.skill_advances.clear();
        }
    }

    fn flush_achievements(&mut self, world: &World) {
        if !self.achievements.is_empty() {
            if let Some(mut awarded) = world.try_resource_mut::<crate::Achievements>() {
                for id in self.achievements.drain(..) {
                    if awarded.award(id) {
                        log::info!("achievement {id} awarded");
                    }
                }
            }
            self.achievements.clear();
        }
    }

    fn flush_misc_stats(&mut self, world: &World) {
        if !self.misc_stats.is_empty() {
            if let Some(mut stats) = world.try_resource_mut::<crate::MiscStats>() {
                for (stat, amount) in self.misc_stats.drain(..) {
                    stats.increment(&stat, amount);
                }
            }
            self.misc_stats.clear();
        }
    }

    fn flush_player_xp(&mut self, world: &World) {
        if !self.player_xp.is_empty() {
            if let Some(mut pending) =
                world.try_resource_mut::<crate::story_manager::PendingPlayerXp>()
            {
                pending.0.append(&mut self.player_xp);
            }
        }
    }

    fn flush_kills(&mut self, world: &World) {
        if !self.kills.is_empty() {
            if let Some(mut pending) =
                world.try_resource_mut::<crate::story_manager::PendingScriptKills>()
            {
                pending.0.append(&mut self.kills);
            }
        }
    }

    fn flush_story_events(&mut self, world: &World) {
        if !self.story_events.is_empty() {
            if let Some(mut pending) =
                world.try_resource_mut::<crate::story_manager::PendingScriptStoryEvents>()
            {
                pending.0.append(&mut self.story_events);
            }
        }
    }

    fn flush_star_map_changes(&mut self, world: &World) {
        if !self.star_map_changes.is_empty() {
            if let Some(mut markers) = world.try_resource_mut::<crate::MapMarkers>() {
                for (marker, visible) in self.star_map_changes.drain(..) {
                    markers.set_star_map_visible(marker, visible);
                }
            }
            self.star_map_changes.clear();
        }
    }

    fn flush_map_markers(&mut self, world: &World) {
        if !self.map_markers.is_empty() {
            if let Some(mut markers) = world.try_resource_mut::<crate::MapMarkers>() {
                for (marker, fast_travel) in std::mem::take(&mut self.map_markers) {
                    markers.add(marker, fast_travel);
                }
            }
        }
    }

    fn flush_relationship_changes(&mut self, world: &World) {
        if !self.relationship_changes.is_empty() {
            if let Some(mut ranks) = world.try_resource_mut::<crate::ScriptedRelationships>() {
                for (pair, rank) in std::mem::take(&mut self.relationship_changes) {
                    ranks.0.insert(pair, rank);
                }
            }
        }
    }

    fn flush_base_protection(&mut self, world: &World) {
        if !self.base_protection.is_empty() {
            if let Some(mut overrides) = world.try_resource_mut::<crate::ActorBaseProtectionOverrides>()
            {
                for (base, protected, value) in std::mem::take(&mut self.base_protection) {
                    overrides.set(base, protected, value);
                }
            }
        }
    }

    fn flush_location_clears(&mut self, world: &World) {
        if !self.location_clears.is_empty() {
            if let Some(mut state) = world.try_resource_mut::<crate::LocationClearState>() {
                for (location, cleared) in std::mem::take(&mut self.location_clears) {
                    state.set_cleared(location, cleared);
                }
            }
        }
    }

    fn flush_activation_blocks(&mut self, world: &World) {
        if !self.activation_blocks.is_empty() {
            if let Some(mut blocks) = world.try_resource_mut::<super::ReferenceActivationBlocks>() {
                for (reference, blocked) in std::mem::take(&mut self.activation_blocks) {
                    blocks.set_blocked(reference, blocked);
                }
            }
        }
    }

    fn flush_alias_overrides(&mut self, world: &World) {
        if !self.alias_overrides.is_empty() {
            if let Some(mut overrides) = world.try_resource_mut::<crate::AliasOverrides>() {
                for (quest, alias_id, value) in std::mem::take(&mut self.alias_overrides) {
                    overrides.set(quest, alias_id, value);
                }
            }
        }
    }

    fn flush_scene_actor_bindings_dirty(&mut self, world: &World) {
        if self.scene_actor_bindings_dirty {
            crate::scene::mark_scene_actor_bindings_dirty(world);
        }
    }

    fn flush_activations(&mut self, world: &World) {
        if !self.activations.is_empty() || !self.native_activations.is_empty() {
            match world.try_resource_mut::<PendingFragmentActivations>() {
                Some(mut pending) => {
                    pending.0.append(&mut self.activations);
                    pending.1.append(&mut self.native_activations);
                }
                None => log::debug!(
                    "fragment Activate dropped: PendingFragmentActivations is unavailable"
                ),
            }
        }
    }

    fn flush_reference_enable_changes(&mut self, world: &World) {
        if !self.reference_enable_changes.is_empty() {
            if let Some(mut state) = world.try_resource_mut::<ReferenceEnableState>() {
                for (form_id, enabled) in self.reference_enable_changes.drain(..) {
                    state.set_enabled(form_id, enabled);
                }
            }
        }
    }

    fn flush_reference_lock_changes(&mut self, world: &World) {
        if !self.reference_lock_changes.is_empty() {
            match world.try_resource_mut::<crate::ReferenceLockState>() {
                Some(mut state) => {
                    for (form_id, change) in self.reference_lock_changes.drain(..) {
                        match change {
                            DeferredLockChange::Locked {
                                lock_level,
                                key_form_id,
                            } => state.set_locked(form_id, lock_level, key_form_id),
                            DeferredLockChange::Unlocked => state.set_unlocked(form_id),
                        }
                    }
                }
                None => log::debug!(
                    "fragment lock change dropped from the persistent ledger: \
                     ReferenceLockState is unavailable — the live component still \
                     changed, but the change will not survive a cell reload (#4136)"
                ),
            }
        }
    }

    fn flush_cinematic_presentation(&mut self, world: &World) {
        if !self.cinematic_presentation.is_empty() {
            if let Some(mut state) = world.try_resource_mut::<crate::CinematicPresentationState>() {
                for effect in self.cinematic_presentation.drain(..) {
                    match effect {
                        DeferredCinematicPresentationEffect::SetSittingRotation(degrees) => {
                            state.sitting_rotation_degrees = degrees;
                        }
                        DeferredCinematicPresentationEffect::RegisterPlayerAnimationEvent {
                            event,
                            quest,
                            image_space_modifiers,
                        } => {
                            state.register_player_animation_event(
                                event,
                                quest,
                                image_space_modifiers,
                            );
                        }
                        DeferredCinematicPresentationEffect::SetInChargen {
                            disable_saving,
                            disable_waiting,
                            show_controls_disabled_message,
                        } => {
                            state.set_in_chargen(
                                disable_saving,
                                disable_waiting,
                                show_controls_disabled_message,
                            );
                        }
                        DeferredCinematicPresentationEffect::ShowRaceMenu => {
                            state.show_race_menu();
                        }
                        DeferredCinematicPresentationEffect::RequestSave { auto } => {
                            state.request_save(auto);
                        }
                    }
                }
            }
        }
    }

    fn flush_faction_changes(&mut self, world: &World) {
        if !self.faction_changes.is_empty() {
            let player = crate::papyrus_demo::PapyrusPlayerEntity::get(world);
            for (actor, faction, change) in std::mem::take(&mut self.faction_changes) {
                // `RemoveFromAllFactions` names no faction: the ledger records
                // each one the actor was in as removed.
                let left: Vec<u32> = match change {
                    crate::FactionChange::RemoveAll => world
                        .get::<byroredux_core::ecs::components::FactionRanks>(actor)
                        .map(|ranks| ranks.0.iter().map(|(faction, _)| *faction).collect())
                        .unwrap_or_default(),
                    _ => Vec::new(),
                };
                let rank = {
                    let Some(mut storage) =
                        world.query_mut::<byroredux_core::ecs::components::FactionRanks>()
                    else {
                        log::debug!("faction change skipped: FactionRanks never registered");
                        continue;
                    };
                    if storage.get_mut(actor).is_none() {
                        storage.insert(actor, byroredux_core::ecs::components::FactionRanks::new());
                    }
                    change.apply(storage.get_mut(actor).expect("present or inserted"), faction)
                };
                // The ledger key: PlayerRef for the player, else the placed
                // reference's FormID. An actor with neither keeps the live
                // edit only (nothing re-derives it under a stable identity).
                let reference = if player == Some(actor) {
                    Some(crate::PLAYER_REFERENCE)
                } else {
                    entity_global_form_id(world, actor)
                };
                match reference {
                    Some(reference) => {
                        if let Some(mut ledger) = world.try_resource_mut::<crate::ScriptedFactionChanges>() {
                            if change == crate::FactionChange::RemoveAll {
                                for faction in left {
                                    ledger.record(reference, faction, None);
                                }
                            } else {
                                ledger.record(reference, faction, rank);
                            }
                        }
                    }
                    None => log::debug!(
                        "faction change on {actor:?} not persisted: the actor has no reference identity"
                    ),
                }
            }
        }
    }

    fn flush_quest_variables(&mut self, world: &World) {
        if !self.quest_variables.is_empty() {
            match world.try_resource_mut::<crate::QuestScriptVariables>() {
                Some(mut variables) => {
                    for (quest, variable, value) in self.quest_variables.drain(..) {
                        variables.set(quest, variable, value);
                    }
                }
                None => {
                    log::debug!(
                        "{} quest-variable write(s) dropped: QuestScriptVariables not installed",
                        self.quest_variables.len()
                    );
                    self.quest_variables.clear();
                }
            }
        }
    }

    fn flush_quest_objects(&mut self, world: &World) {
        if !self.quest_objects.is_empty() {
            match world.try_resource_mut::<crate::QuestScriptObjects>() {
                Some(mut objects) => {
                    for (quest, variable, value) in self.quest_objects.drain(..) {
                        objects.set(quest, variable, value);
                    }
                }
                None => {
                    log::debug!("object script-variable writes dropped: QuestScriptObjects not installed");
                    self.quest_objects.clear();
                }
            }
        }
    }

    fn flush_script_updates(&mut self, world: &World) {
        if !self.script_updates.is_empty() {
            match world.try_resource_mut::<super::ScriptUpdates>() {
                Some(mut updates) => {
                    for (owner, script, request) in self.script_updates.drain(..) {
                        updates.request(owner, &script, request);
                    }
                }
                None => {
                    log::debug!("update registrations dropped: ScriptUpdates not installed");
                    self.script_updates.clear();
                }
            }
        }
    }

    fn flush_faction_relations(&mut self, world: &World) {
        if !self.faction_relations.is_empty() {
            // #4414 — the relation is saved, so it is keyed by portable
            // identity. Resolve under the identity guard alone, then take
            // the relations guard: never both held at once.
            let resolved = match world.try_resource::<crate::LoadOrderIdentity>() {
                Some(identity) => self
                    .faction_relations
                    .drain(..)
                    .filter_map(|(faction, other, reaction, other_reaction)| {
                        let pair = identity.form_ref(faction).zip(identity.form_ref(other));
                        if pair.is_none() {
                            log::warn!(
                                "SetEnemy/SetAlly {faction:08X} -> {other:08X} dropped: a \
                                 faction has no owning plugin in the load order"
                            );
                        }
                        pair.map(|(faction, other)| (faction, other, reaction, other_reaction))
                    })
                    .collect::<Vec<_>>(),
                None => {
                    log::warn!(
                        "{} SetEnemy/SetAlly call(s) dropped: no LoadOrderIdentity",
                        self.faction_relations.len()
                    );
                    self.faction_relations.clear();
                    Vec::new()
                }
            };
            if let Some(mut relations) = world.try_resource_mut::<crate::FactionRelations>() {
                for (faction, other, reaction, other_reaction) in resolved {
                    relations.set_reaction(faction, other, reaction);
                    relations.set_reaction(other, faction, other_reaction);
                }
            }
        }
    }
}

/// A follower-flag write a fragment queued.
#[derive(Debug, Clone, Copy)]
pub(in crate::fragment) enum FollowerChange {
    Teammate { teammate: bool, can_do_favor: bool },
    DoingFavor(bool),
}

/// Execute one fragment as an ordered unit. Any provider barrier is flushed
/// after releasing quest-state guards before the caller starts another
/// independent fragment.
pub(crate) fn apply_fragment_guard_free(
    world: &World,
    effects: &[Effect],
    context: QuestFormId,
    vmad: Option<&ScriptInstanceData>,
) -> Vec<QuestStageAdvanced> {
    apply_fragment_guard_free_as(world, effects, context, vmad, None)
}

/// [`apply_fragment_guard_free`] for a dialogue line's fragment, with the
/// line's `speaker` bound for `ActorRef::Speaker` (M47.2).
pub(crate) fn apply_fragment_guard_free_as(
    world: &World,
    effects: &[Effect],
    context: QuestFormId,
    vmad: Option<&ScriptInstanceData>,
    speaker: Option<EntityId>,
) -> Vec<QuestStageAdvanced> {
    let mut deferred = DeferredFragmentEffects::new(world);
    deferred.speaker = speaker;
    let mut advances = {
        let (mut stages, mut objectives) =
            world.resource_2_mut::<QuestStageState, QuestObjectiveState>();
        apply_effects(
            effects,
            context,
            vmad,
            world,
            &mut stages,
            &mut objectives,
            &mut deferred,
        )
    };
    advances.extend(deferred.apply(world));
    advances
}

/// Consume quest-journal entries produced while one guard-free fragment ran.
/// The journal supplies canonical interleaving (including any host-side stage
/// changes); direct advances are retained only as a loss-recovery fallback.
pub(crate) fn poll_fragment_generated_advances(
    world: &World,
    direct: Vec<QuestStageAdvanced>,
) -> Vec<QuestStageAdvanced> {
    let read = world
        .resource_mut::<QuestStageState>()
        .poll_quest_events(FRAGMENT_QUEST_EVENT_SUBSCRIBER);
    if read.missed_events > 0 {
        log::error!(
            target: "scripting::quest_fragments",
            "fragment subscriber missed {} retained quest transition(s) during guard-free execution",
            read.missed_events
        );
    }
    let mut observed = read
        .events
        .into_iter()
        .map(|sequenced| sequenced.event)
        .collect::<Vec<_>>();
    let mut observed_counts = HashMap::new();
    for event in &observed {
        *observed_counts
            .entry((event.quest, event.previous_stage, event.new_stage))
            .or_insert(0usize) += 1;
    }
    for event in direct {
        let count = observed_counts
            .entry((event.quest, event.previous_stage, event.new_stage))
            .or_default();
        if *count > 0 {
            *count -= 1;
        } else {
            observed.push(event);
        }
    }
    observed
}
