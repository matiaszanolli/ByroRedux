//! The effect interpreter: reference resolution plus `apply_effect` and its
//! quest-scoped and batch wrappers.
//!
//! Split out of `fragment.rs` (#3854).

use super::*;

/// Resolve an effect's [`QuestRef`] to a concrete quest FormID.
/// `Self`/`GetOwningQuest` resolve to the advancing quest; a
/// `Quest Property` resolves through the quest's VMAD (when supplied).
fn resolve_quest(
    via: &QuestRef,
    context: QuestFormId,
    vmad: Option<&ScriptInstanceData>,
) -> Option<QuestFormId> {
    match via {
        QuestRef::SelfRef | QuestRef::OwningQuest => Some(context),
        QuestRef::Property(name) => vmad?
            .scripts
            .iter()
            .find_map(|s| s.object_form_id(name))
            .map(QuestFormId),
    }
}

/// Resolve a `Quest`/`Self`-typed effect's `QuestRef`, logging a `debug`
/// line on failure. Shared by every quest-scoped `apply_effect` arm so
/// the "skip, never guess" contract has one call site.
fn resolve_quest_logged(
    via: &QuestRef,
    context: QuestFormId,
    vmad: Option<&ScriptInstanceData>,
) -> Option<QuestFormId> {
    let quest = resolve_quest(via, context, vmad);
    if quest.is_none() {
        log::debug!("fragment effect skipped: unresolved quest ref {via:?}");
    }
    quest
}

/// Resolve an [`ObjectRef::Property`] to its VMAD-bound FormID. Requires
/// an `Object`-typed property with `alias == -1` — an alias-bound entry
/// (`ReferenceAlias Property`) needs the quest-alias-fill subsystem to
/// resolve correctly, so it declines rather than trusting the raw
/// `form_id` sitting next to a live alias index.
///
/// #2186 — the `alias == -1` check itself now lives in
/// `ScriptInstance::object_form_id`, which `resolve_quest` also uses.
/// This function open-coded the strict match while `object_form_id`
/// stayed lax, so the two sibling resolvers disagreed; sharing one
/// accessor is what keeps them from drifting apart again.
pub(crate) fn resolve_property_form_id(
    vmad: Option<&ScriptInstanceData>,
    name: &str,
) -> Option<u32> {
    vmad?.scripts.iter().find_map(|s| s.object_form_id(name))
}

/// The global form ID of a live entity — the inverse of
/// [`crate::condition::resolve_entity_by_global_form_id`], and keyed the same
/// way (`FormIdPool::resolve(..).local.0`) so the two agree.
///
/// #3278 — needed by [`Effect::Disable`], whose sink
/// ([`ReferenceEnableState`]) is deliberately FormID-keyed rather than
/// entity-keyed so a disable survives its reference's cell being unloaded.
/// Alias-bound receivers resolve to an *entity*, so they have to come back
/// to a form ID to be recorded.
fn entity_global_form_id(
    world: &World,
    entity: byroredux_core::ecs::storage::EntityId,
) -> Option<u32> {
    use byroredux_core::ecs::components::FormIdComponent;
    use byroredux_core::form_id::FormIdPool;
    let component = world.get::<FormIdComponent>(entity)?;
    let pool = world.try_resource::<FormIdPool>()?;
    pool.resolve(component.0).map(|pair| pair.local.0)
}

/// Resolve an [`ObjectRef`] all the way to a live entity. Direct VMAD object
/// properties go through FormID → loaded entity; alias-bound properties go
/// through the owning quest's [`crate::scene::SceneActorBindings`] snapshot
/// (`bindings`, captured by [`DeferredFragmentEffects::new`] before the
/// quest-state guards are taken — #2660 / SCR-D6-NEW11-03 — rather than
/// acquired live here, which would nest a `SceneActorBindings` read inside
/// that scope). `debug`-logs and returns `None` at whichever hop fails — an
/// unresolved object is a runtime data fact (not loaded, not spawned yet),
/// not a shape the fragment failed to understand.
fn resolve_object(
    vmad: Option<&ScriptInstanceData>,
    world: &World,
    context: QuestFormId,
    via: &ObjectRef,
    bindings: &crate::scene::SceneActorBindings,
) -> Option<byroredux_core::ecs::storage::EntityId> {
    let name = via.property_name();
    let Some(value) = vmad?
        .scripts
        .iter()
        .find_map(|script| script.property(name))
        .map(|property| &property.value)
    else {
        log::debug!("fragment effect skipped: object ref '{name}' has no VMAD binding");
        return None;
    };
    let entity = match value {
        PropertyValue::Object { form_id, alias: -1 } => {
            crate::condition::resolve_entity_by_global_form_id(world, *form_id)
        }
        PropertyValue::Object { alias, .. } if *alias >= 0 => {
            bindings.resolve(context, i32::from(*alias))
        }
        _ => None,
    };
    if entity.is_none() {
        log::debug!("fragment effect skipped: object ref '{name}' has no live entity");
    }
    entity
}

fn resolve_actor(
    vmad: Option<&ScriptInstanceData>,
    world: &World,
    context: QuestFormId,
    via: &ActorRef,
    bindings: &crate::scene::SceneActorBindings,
) -> Option<byroredux_core::ecs::storage::EntityId> {
    match via {
        ActorRef::Player => world
            .try_resource::<crate::papyrus_demo::PapyrusPlayerEntity>()
            .map(|player| player.0),
        ActorRef::Object(via) => resolve_object(vmad, world, context, via, bindings),
    }
}

pub(crate) fn actors_3d_loaded(
    vmad: Option<&ScriptInstanceData>,
    world: &World,
    context: QuestFormId,
    actors: &[ActorRef],
    bindings: &crate::scene::SceneActorBindings,
) -> bool {
    actors.iter().all(|actor| {
        resolve_actor(vmad, world, context, actor, bindings)
            .is_some_and(|entity| world.has::<Transform>(entity))
    })
}

fn update_actor_cinematic_state(
    world: &World,
    actor: byroredux_core::ecs::storage::EntityId,
    update: impl FnOnce(&mut crate::ActorCinematicState),
) -> bool {
    let Some(mut states) = world.query_mut::<crate::ActorCinematicState>() else {
        return false;
    };
    if states.get_mut(actor).is_none() {
        states.insert(actor, crate::ActorCinematicState::default());
    }
    let Some(state) = states.get_mut(actor) else {
        return false;
    };
    update(state);
    true
}

fn exit_cart_idle_property(seat: u8) -> Option<&'static str> {
    match seat {
        1 => Some("IdleCartPassengerAExit"),
        2 => Some("IdleCartPassengerBExit"),
        3 => Some("IdleCartPassengerDExit"),
        4 => Some("IdleCartPassengerCExit"),
        5 => Some("IdleCartDriverExit"),
        _ => None,
    }
}

#[derive(Debug)]
enum DeferredCinematicPresentationEffect {
    SetSittingRotation(f32),
    RegisterPlayerAnimationEvent {
        event: crate::CinematicAnimationEvent,
        quest: QuestFormId,
        image_space_modifiers: Vec<crate::ImageSpaceModifierApplication>,
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
#[derive(Debug)]
pub struct DeferredFragmentEffects {
    quest_definitions: Option<crate::QuestDefinitionRegistry>,
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
    cinematic_presentation: Vec<DeferredCinematicPresentationEffect>,
    scene_actor_bindings_dirty: bool,
    /// `(target, activator)` pairs from `Effect::Activate`, handed to
    /// [`PendingFragmentActivations`] so they are delivered at the head of
    /// the *next* frame (#2654) — see that resource's docs.
    activations: Vec<(EntityId, EntityId)>,
    reference_enable_changes: Vec<(u32, bool)>,
    provider_steps: Vec<DeferredProviderFragmentStep>,
}

#[derive(Debug)]
struct DeferredProviderFragmentStep {
    call: crate::translate::effects::FragmentProviderCall,
    context: QuestFormId,
    vmad: Option<ScriptInstanceData>,
    tail: Vec<Effect>,
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
            cinematic_presentation: Vec::new(),
            scene_actor_bindings_dirty: false,
            activations: Vec::new(),
            reference_enable_changes: Vec::new(),
            provider_steps: Vec::new(),
        }
    }

    /// Apply queued mutations after releasing the `QuestStageState` /
    /// `QuestObjectiveState` guards passed to [`apply_effects`].
    pub fn apply(self, world: &World) -> Vec<QuestStageAdvanced> {
        self.apply_at_depth(world, 0)
    }

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
        if self.scene_actor_bindings_dirty {
            crate::scene::mark_scene_actor_bindings_dirty(world);
        }
        if !self.activations.is_empty() {
            match world.try_resource_mut::<PendingFragmentActivations>() {
                Some(mut pending) => pending.0.append(&mut self.activations),
                None => log::debug!(
                    "fragment Activate dropped: PendingFragmentActivations is unavailable"
                ),
            }
        }
        if !self.reference_enable_changes.is_empty() {
            if let Some(mut state) = world.try_resource_mut::<ReferenceEnableState>() {
                for (form_id, enabled) in self.reference_enable_changes.drain(..) {
                    state.set_enabled(form_id, enabled);
                }
            }
        }
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
                    }
                }
            }
        }
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
    let mut deferred = DeferredFragmentEffects::new(world);
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

/// Read one entity's [`Transform`] as an owned copy, holding no guard on
/// return (#3250).
pub(crate) fn copied_transform(world: &World, entity: EntityId) -> Option<Transform> {
    // #3250 — `World::get` returns an owning read guard. Copy the component
    // before another Transform lookup so a queued writer can never strand a
    // recursive read behind the first guard.
    world.get::<Transform>(entity).map(|transform| *transform)
}

/// Apply one effect to the canonical stage/objective state (or, for the
/// object-targeting variants, to the live ECS world). Returns a
/// [`QuestStageAdvanced`] when the effect was a `SetStage` (so the caller
/// can cascade), or `None` otherwise / when a target can't resolve.
///
/// **Nested-lock safety depends on exclusive scheduling.** What this
/// function runs *inside* is one `resource_2_mut::<QuestStageState,
/// QuestObjectiveState>()` taken by [`apply_fragment_guard_free`] — a
/// single TypeId-sorted paired acquisition, scoped to one fragment's
/// effects and dropped before `deferred.apply(world)` runs. It is NOT held
/// across the cascade loop, and `QuestStageFragments` is not held at all:
/// [`quest_fragment_dispatch_system`] clones it before taking any quest
/// resource, precisely to avoid a read→write nested order.
///
/// Everything below is therefore nested under those two guards and nothing
/// else. Listed by where it is taken rather than as a hand count, because
/// the count is what rotted (#3949 — the previous version of this block
/// said "12 component-storage acquisitions" and named three resources that
/// are no longer all held together):
///
///   - **directly in the match arms** — `Globals` (write, `SetGlobalValue`),
///     `PlayerControlState` (write ×3, the `SetPlayerControls` family),
///     `EquipItemCatalog` (read), `SceneRegistry` (read),
///     `PapyrusPlayerEntity` (read), and the component storages
///     `Inventory`, `Transform` (read and write), `GlobalTransform`,
///     `ActorControlState`, `EvaluatePackageRequest`, `HorseTetherState`,
///     `MotionTypeChangeRequest` (×2), `SceneStartRequest`,
///     `SceneStopRequest`, `Locked` (write ×2, the `SetLocked` /
///     `SetLockLevel` pair — #3159)
///   - **via [`resolve_actor`]** — `PapyrusPlayerEntity` (read)
///   - **via [`entity_global_form_id`]** — `FormIdPool` (read)
///   - **via [`update_actor_cinematic_state`]** — `ActorCinematicState`
///
/// so ~15 distinct storage/resource types across ~20 sites in this
/// function plus three helpers. The `FragmentExecutionQueue` write is
/// *not* in this list: it belongs to the latent-continuation path in the
/// dispatch system, outside these guards.
///
/// This is only safe because every system that touches the quest resources
/// is registered `add_exclusive` in `byroredux/src/boot.rs` (parallel
/// systems never run concurrently with an exclusive one), so no other
/// holder can ever form the other half of an ABBA cycle. Adding a new
/// nested component/resource lock here, or moving
/// [`quest_fragment_dispatch_system`] (or any sibling quest-resource
/// system) onto the parallel lane, needs the same analysis re-derived —
/// see SCR-D6-NEW3-03 / #2126, and record the change under the house-rule
/// doc #2270 asks for.
///
/// `SceneActorBindings` used to be part of this residual list too (every
/// `resolve_object` alias lookup read it live) but is now resolved through
/// the pre-lock snapshot in `deferred` instead (#2660) — see
/// [`DeferredFragmentEffects`]'s doc for why a snapshot is safe here.
/// Quest-definition reads use the same snapshot, and presentation /
/// alias-binding *mutations* are appended to it rather than acquired here.
/// Every production caller constructs the batch before taking quest
/// resource guards and applies it only after those guards have dropped
/// (#2269, #2539, #2660).
pub(crate) fn apply_effect(
    effect: &Effect,
    context: QuestFormId,
    vmad: Option<&ScriptInstanceData>,
    world: &World,
    stages: &mut QuestStageState,
    objectives: &mut QuestObjectiveState,
    deferred: &mut DeferredFragmentEffects,
) -> Option<QuestStageAdvanced> {
    match effect {
        Effect::SetGlobalValue { global, value } => {
            let form_id = resolve_property_form_id(vmad, global.property_name())?;
            if let Some(mut globals) = world.try_resource_mut::<crate::Globals>() {
                globals.set(form_id, *value);
            } else {
                log::debug!(
                    "fragment SetValue skipped: Globals resource is unavailable for {form_id:08X}"
                );
            }
            None
        }
        Effect::AddItem {
            container,
            item,
            count,
        } => {
            let container_entity = resolve_object(
                vmad,
                world,
                context,
                container,
                &deferred.scene_actor_bindings,
            )?;
            let item_form_id = resolve_property_form_id(vmad, item.property_name())?;
            let Some(mut inventories) = world.query_mut::<Inventory>() else {
                log::debug!("fragment AddItem skipped: Inventory component never registered");
                return None;
            };
            if inventories.get_mut(container_entity).is_none() {
                // The container doesn't carry an Inventory yet — every
                // object can receive items in Bethesda's runtime, so
                // create one on demand (an interior-mutable insert onto
                // an already-registered storage, not a structural
                // `&mut World` mutation).
                inventories.insert(container_entity, Inventory::new());
            }
            inventories
                .get_mut(container_entity)
                .expect("just present or inserted above")
                .push(ItemStack::new(item_form_id, *count));
            None
        }
        Effect::EquipItem {
            actor,
            item,
            silent: _,
        } => {
            let actor = resolve_actor(vmad, world, context, actor, &deferred.scene_actor_bindings)?;
            let item_form_id = resolve_property_form_id(vmad, item.property_name())?;
            let Some(slot_mask) = world
                .try_resource::<crate::EquipItemCatalog>()
                .and_then(|catalog| catalog.slot_mask(item_form_id))
            else {
                log::debug!(
                    "fragment EquipItem skipped: item {item_form_id:08X} has no armor biped-slot metadata"
                );
                return None;
            };
            let Some((mut inventories, mut equipment)) =
                world.query_2_mut_mut::<Inventory, EquipmentSlots>()
            else {
                log::debug!(
                    "fragment EquipItem skipped: inventory/equipment storage is unavailable"
                );
                return None;
            };
            let Some(inventory) = inventories.get_mut(actor) else {
                log::debug!("fragment EquipItem skipped: actor has no Inventory");
                return None;
            };
            let Some(index) = inventory
                .items
                .iter()
                .position(|stack| stack.base_form_id == item_form_id && stack.count > 0)
                .and_then(|index| u32::try_from(index).ok())
                .map(InventoryIndex)
            else {
                log::debug!(
                    "fragment EquipItem skipped: actor does not carry item {item_form_id:08X}"
                );
                return None;
            };
            if equipment.get_mut(actor).is_none() {
                equipment.insert(actor, EquipmentSlots::new());
            }
            let slots = equipment
                .get_mut(actor)
                .expect("just present or inserted above");
            if slots.is_equipped(index) {
                return None;
            }
            let displaced = slots.equip(slot_mask, index);
            let mut changes = displaced
                .into_iter()
                .filter(|displaced| !slots.is_equipped(*displaced))
                .filter_map(|displaced| {
                    inventory
                        .get(displaced)
                        .map(|stack| crate::EquipmentChange {
                            item_form_id: stack.base_form_id,
                            equipped: false,
                        })
                })
                .collect::<Vec<_>>();
            changes.push(crate::EquipmentChange {
                item_form_id,
                equipped: true,
            });
            drop(inventories);
            drop(equipment);
            crate::emit_equipment_changes(world, actor, changes);
            None
        }
        Effect::MoveTo { moved, destination } => {
            let moved_entity =
                resolve_object(vmad, world, context, moved, &deferred.scene_actor_bindings)?;
            let destination_entity = resolve_object(
                vmad,
                world,
                context,
                destination,
                &deferred.scene_actor_bindings,
            )?;
            let Some(translation) = world
                .get::<GlobalTransform>(destination_entity)
                .map(|gt| gt.translation)
            else {
                log::debug!("fragment MoveTo skipped: destination entity has no GlobalTransform");
                return None;
            };
            let Some(mut transforms) = world.query_mut::<Transform>() else {
                log::debug!("fragment MoveTo skipped: Transform component never registered");
                return None;
            };
            // Unlike AddItem, a "moved" entity with no Transform isn't a
            // container the effect can adopt — it isn't a placed spatial
            // entity at all, so this declines rather than fabricating one.
            let Some(transform) = transforms.get_mut(moved_entity) else {
                log::debug!("fragment MoveTo skipped: moved entity has no Transform");
                return None;
            };
            transform.translation = translation;
            None
        }
        Effect::Disable {
            object,
            fade_out: _,
        }
        | Effect::Enable { object, fade_in: _ } => {
            // #3278 (SCR-D5-2026-08-24-01) — `prim_disable` classifies its
            // receiver through the same `receiver_object` as
            // `AddItem`/`MoveTo`/`EquipItem`, so it can bind to a
            // quest-alias-filled `ObjectReference Property`. Dispatch used
            // only the strict `resolve_property_form_id`, which sees direct
            // VMAD FormID properties and nothing else — so
            // `<AliasBoundMarker>.Disable()` silently declined in exactly the
            // cases where the same alias-bound receiver resolves fine for
            // every sibling effect.
            //
            // Direct-FormID first, alias-aware second: that keeps the cheap,
            // world-free path unchanged for the case that already worked and
            // makes the alias arm strictly additive. `ReferenceEnableState`
            // is FormID-keyed (so the state outlives the reference's cell),
            // hence the trip back through `entity_global_form_id`.
            let form_id = resolve_property_form_id(vmad, object.property_name()).or_else(|| {
                let entity =
                    resolve_object(vmad, world, context, object, &deferred.scene_actor_bindings)?;
                entity_global_form_id(world, entity)
            })?;
            // #3489 — the symmetric counterpart to Disable, sharing this arm
            // (same receiver resolution, same sink) rather than duplicating
            // it: `Effect::Enable` is the only other variant reaching here,
            // so this is exhaustive over the pattern above, not a guess.
            let enabled = matches!(effect, Effect::Enable { .. });
            deferred.reference_enable_changes.push((form_id, enabled));
            None
        }
        Effect::StartScene { scene } | Effect::StopScene { scene } => {
            let scene_form_id = resolve_property_form_id(vmad, scene.property_name())?;
            let scene_entity = world
                .try_resource::<crate::scene::SceneRegistry>()
                .and_then(|registry| registry.scene_entity(scene_form_id));
            if let Some(scene_entity) = scene_entity {
                if matches!(effect, Effect::StartScene { .. }) {
                    if let Some(mut requests) = world.query_mut::<crate::scene::SceneStartRequest>()
                    {
                        requests.insert(scene_entity, crate::scene::SceneStartRequest);
                    }
                } else if let Some(mut requests) =
                    world.query_mut::<crate::scene::SceneStopRequest>()
                {
                    requests.insert(scene_entity, crate::scene::SceneStopRequest);
                }
                return None;
            }

            // `Start`/`Stop` are shared by Scene and Quest in Papyrus, while
            // the decompiled AST does not retain a property's declared type.
            // Resolve the FormID against installed definitions at dispatch.
            let quest = QuestFormId(scene_form_id);
            let (start_up_stage, shut_down_stage) = {
                let definitions = deferred.quest_definitions.as_ref()?;
                if !definitions.contains(quest) {
                    log::debug!(
                        "fragment Start/Stop skipped: '{}' resolved to unknown form {scene_form_id:08X}",
                        scene.property_name()
                    );
                    return None;
                }
                (
                    definitions.start_up_stage(quest),
                    definitions.shut_down_stage(quest),
                )
            };
            if matches!(effect, Effect::StartScene { .. }) {
                let event = stages.start_quest(quest, start_up_stage);
                if event.is_some() {
                    deferred.scene_actor_bindings_dirty = true;
                }
                event
            } else {
                let event = stages
                    .is_running(quest)
                    .then(|| {
                        shut_down_stage.map(|stage| {
                            let previous_stage = stages.set_stage(quest, stage);
                            QuestStageAdvanced {
                                quest,
                                previous_stage,
                                new_stage: stage,
                            }
                        })
                    })
                    .flatten();
                if stages.stop(quest) {
                    deferred.scene_actor_bindings_dirty = true;
                }
                event
            }
        }
        Effect::Activate { target, activator } => {
            let target_entity =
                resolve_object(vmad, world, context, target, &deferred.scene_actor_bindings)?;
            let activator = match activator {
                Some(activator) => resolve_object(
                    vmad,
                    world,
                    context,
                    activator,
                    &deferred.scene_actor_bindings,
                )?,
                None => world
                    .try_resource::<crate::papyrus_demo::PapyrusPlayerEntity>()
                    .map(|player| player.0)
                    .unwrap_or(target_entity),
            };
            // Queue rather than insert: `quest_fragment_dispatch_system` is
            // the LAST producer of `ActivateEvent` in Stage::Update, but
            // three of the four consumers (rumble, quest_advance,
            // two_state_activator) are scheduled earlier and
            // `event_cleanup_system` drains the marker at Stage::Late the
            // same frame — so a directly-inserted marker reached none of
            // them, ever (#2654). `fragment_activation_flush_system` turns
            // this into a real `ActivateEvent` at the head of the next
            // frame, ahead of every consumer.
            deferred.activations.push((target_entity, activator));
            None
        }
        Effect::SetOpen { target, open } => {
            let target_entity =
                resolve_object(vmad, world, context, target, &deferred.scene_actor_bindings)?;
            if !crate::vm_state::set_two_state_open(world, target_entity, *open) {
                log::debug!(
                    "fragment SetOpen skipped: '{}' is not a recognized two-state activator",
                    target.property_name()
                );
            }
            None
        }
        Effect::SetLocked { target, locked } => {
            // #3159 — the removal half that did not exist. `Locked` had one
            // insert (the cell loader's XLOC stamp) and one read (the
            // interaction gate) and nothing that cleared it, so an authored
            // lock was a one-way door for the session and any fragment
            // containing `Lock(false)` declined wholesale — discarding its
            // sibling stage/objective effects too.
            let target_entity =
                resolve_object(vmad, world, context, target, &deferred.scene_actor_bindings)?;
            // Through `query_mut`, not `World::insert`/`remove`: this system
            // holds `&World`, so structural mutation is unavailable, but
            // inserting into and removing from an *existing* storage is not
            // structural. `boot.rs` pre-registers `Locked` so the storage is
            // there even in a session whose cells authored no XLOC at all —
            // otherwise the very first scripted lock would silently no-op.
            let Some(mut locks) = world.query_mut::<Locked>() else {
                log::debug!("fragment Lock skipped: Locked storage never registered");
                return None;
            };
            if *locked {
                // Re-locking something the cell loader never stamped: there is
                // no authored `XLOC` to recover a level or key from, so the
                // lock is recorded at its least-restrictive shape rather than
                // inventing a difficulty. A re-lock of a previously-locked
                // object keeps whatever it already carried.
                if locks.get(target_entity).is_none() {
                    locks.insert(
                        target_entity,
                        Locked {
                            lock_level: 0,
                            key_form_id: None,
                        },
                    );
                }
            } else {
                locks.remove(target_entity);
            }
            None
        }
        Effect::SetLockLevel { target, level } => {
            // Difficulty only — never locks or unlocks (see the effect's
            // doc). An unlocked object has no `Locked` component to carry a
            // level, so this is a no-op there rather than an implicit lock.
            let target_entity =
                resolve_object(vmad, world, context, target, &deferred.scene_actor_bindings)?;
            if let Some(mut locks) = world.query_mut::<Locked>() {
                if let Some(state) = locks.get_mut(target_entity) {
                    state.lock_level = *level;
                } else {
                    log::debug!(
                        "fragment SetLockLevel skipped: '{}' is not locked, so there is \
                         no lock record to set a difficulty on",
                        target.property_name()
                    );
                }
            }
            None
        }
        Effect::SetPlayerRestrained { restrained } => {
            let Some(player) = world
                .try_resource::<crate::papyrus_demo::PapyrusPlayerEntity>()
                .map(|player| player.0)
            else {
                log::debug!("fragment SetRestrained skipped: player entity is unavailable");
                return None;
            };
            let Some(mut states) = world.query_mut::<crate::ActorControlState>() else {
                log::debug!("fragment SetRestrained skipped: actor-control storage is unavailable");
                return None;
            };
            if let Some(state) = states.get_mut(player) {
                state.restrained = *restrained;
            } else {
                states.insert(
                    player,
                    crate::ActorControlState {
                        restrained: *restrained,
                    },
                );
            }
            None
        }
        Effect::SetPlayerControls { enabled, selection } => {
            if let Some(mut controls) = world.try_resource_mut::<crate::PlayerControlState>() {
                controls.apply_selection(*enabled, *selection);
            }
            None
        }
        Effect::SetPlayerAiDriven { ai_driven } => {
            if let Some(mut controls) = world.try_resource_mut::<crate::PlayerControlState>() {
                controls.ai_driven = *ai_driven;
            }
            None
        }
        Effect::SetHudCartMode { cart_mode } => {
            if let Some(mut controls) = world.try_resource_mut::<crate::PlayerControlState>() {
                controls.hud_cart_mode = *cart_mode;
            }
            None
        }
        Effect::PlayIdle { actor, idle } => {
            let actor = resolve_actor(vmad, world, context, actor, &deferred.scene_actor_bindings)?;
            let idle_form_id = resolve_property_form_id(vmad, idle.property_name())?;
            if !update_actor_cinematic_state(world, actor, |state| {
                state.request_idle(idle_form_id);
            }) {
                log::debug!("fragment PlayIdle skipped: cinematic actor storage is unavailable");
            }
            None
        }
        Effect::SetVehicle { actor, vehicle } => {
            let actor = resolve_actor(vmad, world, context, actor, &deferred.scene_actor_bindings)?;
            let vehicle = match vehicle {
                Some(vehicle) => Some(resolve_object(
                    vmad,
                    world,
                    context,
                    vehicle,
                    &deferred.scene_actor_bindings,
                )?),
                None => None,
            };
            let relative_pose = vehicle.and_then(|vehicle| {
                let actor_transform = copied_transform(world, actor)?;
                let vehicle_transform = copied_transform(world, vehicle)?;
                if vehicle_transform.scale.abs() <= f32::EPSILON {
                    return None;
                }
                let inverse_rotation = vehicle_transform.rotation.conjugate();
                Some((
                    inverse_rotation
                        * (actor_transform.translation - vehicle_transform.translation)
                        / vehicle_transform.scale,
                    inverse_rotation * actor_transform.rotation,
                ))
            });
            if !update_actor_cinematic_state(world, actor, |state| {
                state.vehicle = vehicle;
                if vehicle.is_some() {
                    state.cart_seat = None;
                }
                state.vehicle_local_translation = relative_pose.map(|pose| pose.0);
                state.vehicle_local_rotation = relative_pose.map(|pose| pose.1);
            }) {
                log::debug!("fragment SetVehicle skipped: cinematic actor storage is unavailable");
            }
            None
        }
        Effect::TetherToHorse { cart, horse } => {
            let cart = resolve_object(vmad, world, context, cart, &deferred.scene_actor_bindings)?;
            let horse =
                resolve_object(vmad, world, context, horse, &deferred.scene_actor_bindings)?;
            let Some(cart_transform) = copied_transform(world, cart) else {
                log::debug!("fragment TetherToHorse skipped: cart has no Transform");
                return None;
            };
            let Some(horse_transform) = copied_transform(world, horse) else {
                log::debug!("fragment TetherToHorse skipped: horse has no Transform");
                return None;
            };
            if horse_transform.scale.abs() <= f32::EPSILON {
                log::debug!("fragment TetherToHorse skipped: horse has zero scale");
                return None;
            }
            let inverse_rotation = horse_transform.rotation.conjugate();
            let state = crate::HorseTetherState {
                horse,
                horse_local_translation: inverse_rotation
                    * (cart_transform.translation - horse_transform.translation)
                    / horse_transform.scale,
                horse_local_rotation: inverse_rotation * cart_transform.rotation,
                route_target_form_id: None,
            };
            if let Some(mut tethers) = world.query_mut::<crate::HorseTetherState>() {
                tethers.insert(cart, state);
                log::info!("TetherToHorse attached cart entity {cart} to horse entity {horse}");
            }
            // The native engine realizes this relation through Havok. Redux's
            // deterministic transform follower needs the cart to stop being
            // pulled back from a dynamic Rapier body after PostUpdate, so make
            // the tethered cart keyframed through the same one-shot bridge as
            // authored SetMotionType calls.
            if let Some(mut requests) = world.query_mut::<crate::MotionTypeChangeRequest>() {
                requests.insert(
                    cart,
                    crate::MotionTypeChangeRequest {
                        motion_type: byroredux_core::ecs::components::MotionType::Keyframed,
                        allow_activate: true,
                    },
                );
            }
            None
        }
        Effect::SetMotionType {
            target,
            motion_type,
            allow_activate,
        } => {
            let target =
                resolve_object(vmad, world, context, target, &deferred.scene_actor_bindings)?;
            if let Some(mut requests) = world.query_mut::<crate::MotionTypeChangeRequest>() {
                requests.insert(
                    target,
                    crate::MotionTypeChangeRequest {
                        motion_type: *motion_type,
                        allow_activate: *allow_activate,
                    },
                );
            }
            None
        }
        Effect::SetSittingRotation { degrees } => {
            deferred.cinematic_presentation.push(
                DeferredCinematicPresentationEffect::SetSittingRotation(*degrees),
            );
            None
        }
        Effect::ExitCart { actor, seat } => {
            let actor =
                resolve_object(vmad, world, context, actor, &deferred.scene_actor_bindings)?;
            let idle_form_id = exit_cart_idle_property(*seat)
                .and_then(|property| resolve_property_form_id(vmad, property));
            let attachment = world
                .get::<crate::ActorCinematicState>(actor)
                .and_then(|state| Some((state.vehicle?, state.vehicle_local_rotation?)));
            let exit_root_motion_rotation = world.query::<Transform>().and_then(|transforms| {
                attachment
                    .and_then(|(vehicle, local_rotation)| {
                        transforms
                            .get(vehicle)
                            .map(|vehicle| vehicle.rotation * local_rotation)
                    })
                    .or_else(|| transforms.get(actor).map(|actor| actor.rotation))
            });
            if !update_actor_cinematic_state(world, actor, |state| {
                state.vehicle = None;
                state.vehicle_local_translation = None;
                state.vehicle_local_rotation = None;
                state.cart_seat = Some(*seat);
                state.awaited_event = Some(crate::CinematicAnimationEvent::ExitCartEnd);
                state.exit_root_motion_rotation = exit_root_motion_rotation;
                if let Some(idle_form_id) = idle_form_id {
                    state.request_idle(idle_form_id);
                }
            }) {
                log::debug!("fragment ExitCart skipped: cinematic actor storage is unavailable");
            }
            None
        }
        Effect::RegisterPlayerAnimationEvent { event } => {
            let image_space_modifiers = match event {
                crate::CinematicAnimationEvent::PlayImod => {
                    ["PlayerAlduinIMOD", "CGDragonAttackBlurLong"]
                        .into_iter()
                        .filter_map(|property| resolve_property_form_id(vmad, property))
                        .map(|form_id| crate::ImageSpaceModifierApplication {
                            form_id,
                            strength: 1.0,
                        })
                        .collect()
                }
                crate::CinematicAnimationEvent::IdleFurnitureExit
                | crate::CinematicAnimationEvent::ExitCartEnd => Vec::new(),
            };
            deferred.cinematic_presentation.push(
                DeferredCinematicPresentationEffect::RegisterPlayerAnimationEvent {
                    event: *event,
                    quest: context,
                    image_space_modifiers,
                },
            );
            None
        }
        Effect::EvaluatePackage { actor } => {
            let actor =
                resolve_object(vmad, world, context, actor, &deferred.scene_actor_bindings)?;
            if let Some(mut requests) = world.query_mut::<crate::EvaluatePackageRequest>() {
                requests.insert(actor, crate::EvaluatePackageRequest);
            }
            None
        }
        Effect::Wait { .. } | Effect::WaitForActors3DLoaded { .. } => None,
        Effect::Conditional { .. } => {
            unreachable!("conditional effects are expanded by apply_effects")
        }
        _ => apply_quest_scoped_effect(effect, context, vmad, stages, objectives, deferred),
    }
}

/// The original quest-scoped effect arms (`SetStage`/objectives), split
/// out so [`apply_effect`]'s object-targeting arms — which need `world`
/// but no `QuestRef` resolution — read cleanly above.
fn apply_quest_scoped_effect(
    effect: &Effect,
    context: QuestFormId,
    vmad: Option<&ScriptInstanceData>,
    stages: &mut QuestStageState,
    objectives: &mut QuestObjectiveState,
    deferred: &mut DeferredFragmentEffects,
) -> Option<QuestStageAdvanced> {
    match effect {
        Effect::SetStage { quest, stage } => {
            let quest = resolve_quest_logged(quest, context, vmad)?;
            let allow_repeated = deferred
                .quest_definitions
                .as_ref()
                .is_some_and(|definitions| definitions.allows_repeated_stages(quest));
            if !allow_repeated && stages.get_stage_done(quest, *stage) {
                return None;
            }
            let was_started = stages.is_started(quest);
            let previous_stage = stages.set_stage(quest, *stage);
            if !was_started {
                deferred.scene_actor_bindings_dirty = true;
            }
            Some(QuestStageAdvanced {
                quest,
                previous_stage,
                new_stage: *stage,
            })
        }
        Effect::StartQuest { quest } => {
            let quest = resolve_quest_logged(quest, context, vmad)?;
            let start_up_stage = deferred
                .quest_definitions
                .as_ref()
                .and_then(|definitions| definitions.start_up_stage(quest));
            let event = stages.start_quest(quest, start_up_stage);
            if event.is_some() {
                deferred.scene_actor_bindings_dirty = true;
            }
            event
        }
        Effect::StopQuest { quest } => {
            let quest = resolve_quest_logged(quest, context, vmad)?;
            let shut_down_stage = deferred
                .quest_definitions
                .as_ref()
                .and_then(|definitions| definitions.shut_down_stage(quest));
            let event = stages
                .is_running(quest)
                .then(|| {
                    shut_down_stage.map(|stage| {
                        let previous_stage = stages.set_stage(quest, stage);
                        QuestStageAdvanced {
                            quest,
                            previous_stage,
                            new_stage: stage,
                        }
                    })
                })
                .flatten();
            if stages.stop(quest) {
                deferred.scene_actor_bindings_dirty = true;
            }
            event
        }
        Effect::CompleteQuest { quest } => {
            let quest = resolve_quest_logged(quest, context, vmad)?;
            if stages.complete(quest) {
                deferred.scene_actor_bindings_dirty = true;
            }
            None
        }
        Effect::ResetQuest { quest } => {
            let quest = resolve_quest_logged(quest, context, vmad)?;
            stages.reset(quest);
            objectives.reset(quest);
            deferred.scene_actor_bindings_dirty = true;
            None
        }
        Effect::SetQuestActive { quest, active } => {
            let quest = resolve_quest_logged(quest, context, vmad)?;
            stages.set_active(quest, *active);
            None
        }
        Effect::SetObjectiveDisplayed {
            quest,
            objective,
            displayed,
        } => {
            let quest = resolve_quest_logged(quest, context, vmad)?;
            objectives.set_displayed(quest, *objective, *displayed);
            None
        }
        Effect::SetObjectiveCompleted {
            quest,
            objective,
            completed,
        } => {
            let quest = resolve_quest_logged(quest, context, vmad)?;
            objectives.set_completed(quest, *objective, *completed);
            None
        }
        Effect::SetObjectiveFailed {
            quest,
            objective,
            failed,
        } => {
            let quest = resolve_quest_logged(quest, context, vmad)?;
            objectives.set_failed(quest, *objective, *failed);
            None
        }
        Effect::CompleteAllObjectives { quest } => {
            let quest = resolve_quest_logged(quest, context, vmad)?;
            let authored = deferred
                .quest_definitions
                .as_ref()
                .map(|definitions| definitions.objectives(quest).to_vec())
                .unwrap_or_default();
            if authored.is_empty() {
                objectives.complete_all(quest);
            } else {
                objectives.complete_all_authored(quest, authored);
            }
            None
        }
        Effect::FailAllObjectives { quest } => {
            let quest = resolve_quest_logged(quest, context, vmad)?;
            let authored = deferred
                .quest_definitions
                .as_ref()
                .map(|definitions| definitions.objectives(quest).to_vec())
                .unwrap_or_default();
            if authored.is_empty() {
                objectives.fail_all(quest);
            } else {
                objectives.fail_all_authored(quest, authored);
            }
            None
        }
        Effect::Conditional { .. }
        | Effect::SetGlobalValue { .. }
        | Effect::AddItem { .. }
        | Effect::EquipItem { .. }
        | Effect::MoveTo { .. }
        | Effect::Disable { .. }
        | Effect::Enable { .. }
        | Effect::StartScene { .. }
        | Effect::StopScene { .. }
        | Effect::Activate { .. }
        | Effect::SetOpen { .. }
        | Effect::SetLocked { .. }
        | Effect::SetLockLevel { .. }
        | Effect::SetPlayerRestrained { .. }
        | Effect::SetPlayerControls { .. }
        | Effect::SetPlayerAiDriven { .. }
        | Effect::SetHudCartMode { .. }
        | Effect::PlayIdle { .. }
        | Effect::SetVehicle { .. }
        | Effect::TetherToHorse { .. }
        | Effect::SetMotionType { .. }
        | Effect::SetSittingRotation { .. }
        | Effect::ExitCart { .. }
        | Effect::RegisterPlayerAnimationEvent { .. }
        | Effect::EvaluatePackage { .. }
        | Effect::Wait { .. }
        | Effect::WaitForActors3DLoaded { .. }
        | Effect::ProviderCall(_) => {
            unreachable!("object-targeting effects are handled by apply_effect directly")
        }
    }
}

/// Apply a whole fragment's effects, returning the chained
/// [`QuestStageAdvanced`]s its `SetStage`s produced. Resource reads use the
/// pre-lock snapshot in `deferred`; queued mutations must be applied only after
/// releasing the quest-state guards passed here.
pub fn apply_effects(
    effects: &[Effect],
    context: QuestFormId,
    vmad: Option<&ScriptInstanceData>,
    world: &World,
    stages: &mut QuestStageState,
    objectives: &mut QuestObjectiveState,
    deferred: &mut DeferredFragmentEffects,
) -> Vec<QuestStageAdvanced> {
    let mut advances = Vec::new();
    // #3935 — an explicit cursor stack, not recursion. `Effect::Conditional`
    // used to build `branch ++ effects[index + 1..]` and call `apply_effects`
    // on it, so a fragment with N *sequential* conditionals cost N stack
    // frames and cloned the whole remaining program at each one — O(N^2)
    // live `Effect` clones. `MAX_CONDITIONAL_DEPTH` bounds conditional
    // *nesting* at lowering, not the sequential count, and a `.pex` function
    // may carry 65 535 instructions, so N was bounded only by the wire
    // format. Descending now pushes two borrowed slices (the branch and the
    // rest of the current program) and loops; nothing is cloned, and the
    // stack holds slices, not frames.
    let mut pending: Vec<&[Effect]> = Vec::new();
    let mut current: &[Effect] = effects;
    loop {
        let mut halted = false;
        for (index, effect) in current.iter().enumerate() {
            if let Effect::ProviderCall(call) = effect {
                deferred.provider_steps.push(DeferredProviderFragmentStep {
                    call: call.clone(),
                    context,
                    vmad: vmad.cloned(),
                    tail: remaining_program(current, index, &pending),
                });
                halted = true;
                break;
            }
            if let Effect::Conditional {
                guards,
                then_effects,
                else_effects,
            } = effect
            {
                // #3785 — `is_some_and` collapsed "the guard evaluated false" and
                // "the guard's quest ref could not be resolved at all" into the
                // same `false`, and unlike every sibling `resolve_quest_logged`
                // caller (which simply skips the one effect via `?`), `false`
                // here is NOT inert — it selects `else_effects` and runs them.
                // An unresolvable guard must decline the WHOLE Conditional
                // (neither branch), matching the decline-on-unmodeled-reference
                // invariant every other site already follows.
                let mut resolved = true;
                let passes = guards.iter().all(|guard| {
                    match resolve_quest_logged(&guard.quest, context, vmad) {
                        Some(quest) => stages.get_stage_done(quest, guard.stage) == guard.done,
                        None => {
                            resolved = false;
                            false
                        }
                    }
                });
                if !resolved {
                    // `resolve_quest_logged` already emitted a debug line per
                    // unresolved guard (correct for its inert callers); this is
                    // the one site where the consequence is a chosen branch, so
                    // it gets its own louder diagnostic.
                    log::warn!(
                        "fragment effect: Conditional guard's quest ref could not be resolved — \
                     declining the whole Conditional (neither then_effects nor else_effects run)"
                    );
                    continue;
                }
                let branch = if passes { then_effects } else { else_effects };
                // Push the continuation first, then the branch: the stack pops
                // the branch next, so the visible order is still
                // `branch ++ rest-of-program`, exactly as the flattened
                // recursion produced.
                pending.push(&current[index + 1..]);
                pending.push(branch.as_slice());
                break;
            }
            let suspension = match effect {
                Effect::Wait { seconds } => Some((*seconds, FragmentResumeCondition::DelayElapsed)),
                Effect::WaitForActors3DLoaded {
                    actors,
                    poll_seconds,
                } if !actors_3d_loaded(
                    vmad,
                    world,
                    context,
                    actors,
                    &deferred.scene_actor_bindings,
                ) =>
                {
                    Some((
                        *poll_seconds,
                        FragmentResumeCondition::Actors3DLoaded {
                            actors: actors.clone(),
                            poll_seconds: *poll_seconds,
                            elapsed_seconds: 0.0,
                        },
                    ))
                }
                Effect::WaitForActors3DLoaded { .. } => None,
                _ => None,
            };
            if let Some((remaining_seconds, resume_when)) = suspension {
                let tail = remaining_program(current, index, &pending);
                if !tail.is_empty() {
                    if let Some(mut queue) = world.try_resource_mut::<FragmentExecutionQueue>() {
                        queue.pending.push(PendingFragmentExecution {
                            context,
                            vmad: vmad.cloned(),
                            effects: tail,
                            remaining_seconds,
                            resume_when,
                        });
                    } else {
                        log::debug!(
                            "fragment continuation dropped: FragmentExecutionQueue is unavailable"
                        );
                    }
                }
                halted = true;
                break;
            }
            if matches!(effect, Effect::WaitForActors3DLoaded { .. }) {
                continue;
            }
            if let Some(advance) =
                apply_effect(effect, context, vmad, world, stages, objectives, deferred)
            {
                advances.push(advance);
            }
        }
        if halted {
            break;
        }
        // Either the current slice ran to its end, or a Conditional just
        // pushed its branch — both continue at the top of the stack.
        match pending.pop() {
            Some(next) => current = next,
            None => break,
        }
    }
    advances
}

/// The program still to run after `current[index]`: the rest of the current
/// slice followed by everything still on the cursor stack, in execution
/// order (#3935).
///
/// A suspension or provider barrier hands its continuation to another frame
/// (`FragmentExecutionQueue` / `DeferredFragmentEffects`), which outlives
/// the borrowed slices, so this is the one place the remainder is cloned —
/// once, at the point it actually has to be owned. `pending` is a stack, so
/// its execution order is back-to-front.
fn remaining_program(current: &[Effect], index: usize, pending: &[&[Effect]]) -> Vec<Effect> {
    let mut tail = current[index + 1..].to_vec();
    for slice in pending.iter().rev() {
        tail.extend_from_slice(slice);
    }
    tail
}
