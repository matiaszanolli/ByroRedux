//! ECS systems and registration: activation flush, continuation, and the
//! scene/quest fragment dispatchers.
//!
//! Split out of `fragment.rs` (#3854).

use super::*;

/// Deliver queued fragment activations as `ActivateEvent` markers.
///
/// Must be scheduled in `Stage::Update` **before** every `ActivateEvent`
/// consumer; see [`PendingFragmentActivations`].
pub fn fragment_activation_flush_system(world: &World, _dt: f32) {
    let pending: Vec<(EntityId, EntityId)> = {
        let Some(mut queue) = world.try_resource_mut::<PendingFragmentActivations>() else {
            return;
        };
        if queue.0.is_empty() {
            return;
        }
        std::mem::take(&mut queue.0)
    };
    let Some(mut events) = world.query_mut::<crate::ActivateEvent>() else {
        log::debug!("fragment Activate skipped: ActivateEvent component never registered");
        return;
    };
    for (target, activator) in pending {
        events.insert(target, crate::ActivateEvent { activator });
    }
}

/// Tick latent fragment continuations and execute every tail whose
/// `Utility.Wait` has elapsed. A resumed tail can encounter another wait and
/// requeue itself through [`apply_effects`].
pub fn fragment_continuation_system(world: &World, dt: f32) {
    // #2660 (SCR-D6-NEW11-03) — constructed up front (before the resume-
    // condition loop below, which needs the `SceneActorBindings` snapshot
    // for its own `actors_3d_loaded` check) rather than only once `ready`
    // is known non-empty. This snapshot is also needed on frames where every
    // continuation remains blocked.
    let snapshot = DeferredFragmentEffects::new(world);
    let mut ready = Vec::new();
    let pending = {
        let Some(mut queue) = world.try_resource_mut::<FragmentExecutionQueue>() else {
            return;
        };
        std::mem::take(&mut queue.pending)
    };
    let mut still_pending = Vec::with_capacity(pending.len());
    for mut pending in pending {
        pending.remaining_seconds -= dt.max(0.0);
        if pending.remaining_seconds > 0.0 {
            still_pending.push(pending);
            continue;
        }
        match &mut pending.resume_when {
            FragmentResumeCondition::DelayElapsed => ready.push(pending),
            FragmentResumeCondition::Actors3DLoaded {
                actors,
                poll_seconds,
                elapsed_seconds,
            } => {
                if actors_3d_loaded(
                    pending.vmad.as_ref(),
                    world,
                    pending.context,
                    actors,
                    &snapshot.scene_actor_bindings,
                ) {
                    ready.push(pending);
                } else {
                    *elapsed_seconds += *poll_seconds;
                    // #2288 (SCR-D6-NEW5-02) — give up once the total wait
                    // exceeds the cap instead of re-arming forever. Matches
                    // the crate's "skip, never guess" contract: the tail is
                    // declined outright (dropped, not pushed to either
                    // queue), the same choice `MAX_CASCADE` makes for a
                    // runaway `SetStage` cascade in the same file.
                    if *elapsed_seconds > MAX_ACTORS_3D_LOADED_WAIT_SECONDS {
                        log::warn!(
                            "fragment continuation dropped: WaitForActors3DLoaded exceeded \
                             {MAX_ACTORS_3D_LOADED_WAIT_SECONDS}s with actors still unresolved \
                             (context quest {:?})",
                            pending.context
                        );
                        continue;
                    }
                    pending.remaining_seconds = *poll_seconds;
                    still_pending.push(pending);
                }
            }
        }
    }
    if !still_pending.is_empty() {
        if let Some(mut queue) = world.try_resource_mut::<FragmentExecutionQueue>() {
            queue.pending.extend(still_pending);
        }
    }
    if ready.is_empty() {
        return;
    }

    let mut emitted = Vec::new();
    for pending in ready {
        emitted.extend(apply_fragment_guard_free(
            world,
            &pending.effects,
            pending.context,
            pending.vmad.as_ref(),
        ));
    }

    if emitted.is_empty() {
        return;
    }
    let Some(player_entity) = world
        .try_resource::<crate::papyrus_demo::PapyrusPlayerEntity>()
        .map(|player| player.0)
    else {
        return;
    };
    crate::quest_stages::push_quest_stage_advances(world, player_entity, emitted);
}

/// Maximum stage-fragment cascade depth in one dispatch pass — a fragment
/// `SetStage`-ing the next stage runs that stage's fragment too. The cap
/// is a backstop against a cyclic SetStage chain (a fragment that sets a
/// stage whose fragment sets it back); real quest chains are short.
pub(crate) const MAX_CASCADE: usize = 64;

/// Register the fragment-dispatch resources. Both are empty
/// default-constructible runtime stores (unlike `PapyrusPlayerEntity` /
/// `QuestStageState`, which carry per-app-instance state and stay
/// caller-inserted), so initialising them at world-init is safe and
/// keeps [`quest_fragment_dispatch_system`] panic-free out of the box.
pub fn register(world: &mut World) {
    world.register::<Inventory>();
    world.register::<EquipmentSlots>();
    world.insert_resource(QuestStageFragments::default());
    world.insert_resource(SceneFragments::default());
    world.insert_resource(FragmentExecutionQueue::default());
    world.insert_resource(PendingFragmentActivations::default());
    world.insert_resource(ReferenceEnableState::default());
    world.insert_resource(QuestObjectiveState::default());
}

/// Execute the lowered Papyrus fragments emitted by scene playback this
/// frame. Quest advances enter the canonical journal immediately, allowing
/// the later quest-fragment dispatcher to cascade the corresponding QUST
/// fragment in the same update.
pub fn scene_fragment_dispatch_system(world: &World, _dt: f32) {
    let invocations: Vec<crate::scene::SceneFragmentInvocation> = world
        .query::<crate::scene::SceneFragmentInvocationBatch>()
        .map(|query| {
            query
                .iter()
                .flat_map(|(_, batch)| batch.0.iter().cloned())
                .collect()
        })
        .unwrap_or_default();
    if invocations.is_empty() {
        return;
    }
    let fragments = world.resource::<SceneFragments>().clone();
    if fragments.is_empty() {
        return;
    }

    let mut advances = Vec::new();
    for invocation in invocations {
        let Some(fragment) = fragments.get(invocation.scene_form_id, invocation.event) else {
            continue;
        };
        advances.extend(apply_fragment_guard_free(
            world,
            &fragment.effects,
            fragment.context,
            fragment.vmad.as_ref(),
        ));
    }
    if advances.is_empty() {
        return;
    }

    // #3580 — copy the entity out and DROP the `PapyrusPlayerEntity` guard before
    // `push_quest_stage_advances` acquires the `QuestStageAdvancedBatch`
    // storage. Binding the guard to a `let ... else` local kept it alive
    // across that call, recording `PapyrusPlayerEntity -> QuestStageAdvancedBatch`
    // and closing a ring with the other two edges the quest systems record.
    // Every other `PapyrusPlayerEntity` site in the crate already copies the id out
    // of a statement-scoped temporary; this one did not.
    let Some(player) = world
        .try_resource::<crate::papyrus_demo::PapyrusPlayerEntity>()
        .map(|player| player.0)
    else {
        return;
    };
    crate::quest_stages::push_quest_stage_advances(world, player, advances);
}

/// Consume [`QuestStageAdvanced`] markers and run the matching
/// `(quest, stage)` fragments, cascading any `SetStage`s they perform
/// (bounded by [`MAX_CASCADE`]). Runs after `quest_advance_system` (which
/// emits the initial markers) and before end-of-frame cleanup.
///
/// Effect resolution passes the quest's own registered VMAD (see
/// [`QuestStageFragments::insert_vmad`]), when one was registered, so
/// `Self`/owning-quest-targeted effects always apply and a cross-quest
/// `Property`-targeted effect resolves too, as long as the named property
/// is an `Object`-typed binding on the quest's own VMAD. Object, scene,
/// player-control, package, and cinematic effects apply directly against the
/// live ECS world; latent tails are handed to [`FragmentExecutionQueue`].
/// Unrecognized operations still decline the whole fragment at lowering. The
/// table is empty (and this a no-op) on loads without `--scripts-bsa` or on
/// pre-Papyrus games.
pub fn quest_fragment_dispatch_system(world: &World) {
    // Snapshot compatibility ingress before taking resource locks. The
    // sequenced journal below is authoritative; batches remain accepted for
    // direct tests/tools and are deduplicated against journal entries.
    let mut legacy_events = Vec::new();
    if let Some(markers) = world.query::<QuestStageAdvancedBatch>() {
        for (_entity, batch) in markers.iter() {
            legacy_events.extend(batch.0.iter().copied());
        }
    }

    // Static fragment data is cloned before taking mutable quest resources.
    // This avoids a read→write nested resource-lock order and lets the paired
    // mutable resources use the ECS's TypeId-sorted deadlock-safe API.
    let frags = world.resource::<QuestStageFragments>().clone();
    // Do not claim the destructive quest-event journal when there is no
    // fragment table to consume it. The next install/population pass must
    // still be able to dispatch these transitions (#3012).
    if frags.is_empty() {
        return;
    }

    let mut chained: Vec<QuestStageAdvanced> = Vec::new();
    let mut cascade_steps = 0usize;
    // The boolean distinguishes authored ingress from a SetStage emitted
    // by a fragment. The cascade cap must constrain only the latter: a
    // Skyrim bootstrap can legitimately deliver hundreds of independent
    // Start Game Enabled quest events in one tick.
    let mut queue: VecDeque<(QuestFormId, u16, bool)> = VecDeque::new();
    // Compatibility batches mirror journal events but carry no sequence.
    // Count mirrors as a multiset: all sequenced journal commits remain in
    // order, including two legitimate identical transitions, and only the
    // corresponding unsequenced copies are suppressed.
    let mut journal_mirrors: HashMap<(QuestFormId, u16, u16), usize> = HashMap::new();
    {
        let mut stages = world.resource_mut::<QuestStageState>();
        let journal_read = stages.poll_quest_events(FRAGMENT_QUEST_EVENT_SUBSCRIBER);
        if journal_read.missed_events > 0 {
            log::error!(
                target: "scripting::quest_fragments",
                "fragment subscriber missed {} retained quest transition(s); \
                 canonical quest state remains valid but skipped fragment effects cannot be reconstructed",
                journal_read.missed_events
            );
        }
        for sequenced in journal_read.events {
            let event = sequenced.event;
            *journal_mirrors
                .entry((event.quest, event.previous_stage, event.new_stage))
                .or_default() += 1;
            queue.push_back((event.quest, event.new_stage, false));
        }
        for event in legacy_events {
            let key = (event.quest, event.previous_stage, event.new_stage);
            if let Some(mirrors) = journal_mirrors.get_mut(&key) {
                if *mirrors > 0 {
                    *mirrors -= 1;
                    continue;
                }
            }
            queue.push_back((event.quest, event.new_stage, false));
        }
    }

    if queue.is_empty() {
        return;
    }

    while let Some((quest, stage, is_cascade)) = queue.pop_front() {
        if is_cascade {
            cascade_steps += 1;
        }
        if cascade_steps > MAX_CASCADE {
            log::warn!(
                "quest fragment cascade exceeded {MAX_CASCADE} steps at quest {:?} stage {stage}; \
                     stopping (possible cyclic SetStage)",
                quest
            );
            break;
        }
        let Some(effects) = frags.get(quest, stage) else {
            continue;
        };
        let direct = apply_fragment_guard_free(world, effects, quest, frags.vmad(quest));
        let advances = poll_fragment_generated_advances(world, direct);
        for adv in advances {
            // Only cascade genuine transitions (skip a no-op re-set of
            // the same stage to avoid trivial self-loops). #2124 — this
            // must compare `adv`'s own previous/new stage, not the
            // *currently-dispatching* fragment's `(quest, stage)` pair:
            // comparing against `stage` alone let a different quest's
            // genuine transition collide (false negative, silently
            // dropped) or a same-fragment double-`SetStage` re-queue
            // (false positive, duplicate effect application) whenever
            // `adv.new_stage` happened to numerically equal `stage`.
            if adv.previous_stage != adv.new_stage {
                queue.push_back((adv.quest, adv.new_stage, true));
            }
            chained.push(adv);
        }
    }

    // Emit markers for the chained advances so other consumers (journal
    // UI, further-frame dispatch) observe them. Co-opts the same
    // player-entity sink quest_advance_system uses.
    //
    // #1864 / SCR-D7-NEW-01 — insert the whole batch ONCE. A single
    // `apply_effects` call (let alone the whole cascade) can produce >1
    // chained advance; looping `insert()` onto this one shared sink entity
    // would silently collapse every advance but the last.
    if chained.is_empty() {
        return;
    }
    let player_entity = world
        .resource::<crate::papyrus_demo::PapyrusPlayerEntity>()
        .0;
    // #3277 — was a bare `insert()`, the one non-defensive writer of the six.
    // Harmless only while this system was the last same-frame producer in the
    // schedule; `quest_alias_readiness_stage_system` and
    // `scene_fragment_dispatch_system` now run immediately before it.
    crate::quest_stages::push_quest_stage_advances(world, player_entity, chained);
}
