//! Target resolvers (quest, object, actor) and cinematic-state helpers. (#5468, split from `effects.rs`)

use super::*;

/// Resolve an effect's [`QuestRef`] to a concrete quest FormID.
/// `Self`/`GetOwningQuest` resolve to the advancing quest; a
/// `Quest Property` resolves through the quest's VMAD (when supplied).
pub(super) fn resolve_quest(
    via: &QuestRef,
    context: QuestFormId,
    vmad: Option<&ScriptInstanceData>,
) -> Option<QuestFormId> {
    match via {
        QuestRef::SelfRef | QuestRef::OwningQuest => Some(context),
        // M47.2 — through the `::Name_var` normalization `ObjectRef` already
        // had; a decompiled fragment reads an auto-property through its
        // backing variable.
        QuestRef::Property(name) => vmad?
            .scripts
            .iter()
            .find_map(|s| s.object_form_id(crate::translate::compose::vmad_property_name(name)))
            .map(QuestFormId),
    }
}

/// Resolve a `Quest`/`Self`-typed effect's `QuestRef`, logging a `debug`
/// line on failure. Shared by every quest-scoped `apply_effect` arm so
/// the "skip, never guess" contract has one call site.
pub(super) fn resolve_quest_logged(
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

/// The number of items an [`crate::translate::effects::ItemCount`] names: the
/// literal, or the global's / run-time value truncated toward zero (Papyrus's
/// `Float` → `Int` cast). `None` — the effect skips — for an unresolved read
/// or a negative value.
pub(super) fn resolve_item_count(
    count: &crate::translate::effects::ItemCount,
    context: QuestFormId,
    vmad: Option<&ScriptInstanceData>,
    world: &World,
    stages: &QuestStageState,
    deferred: &DeferredFragmentEffects,
) -> Option<u32> {
    use crate::translate::effects::ItemCount;
    let value = match count {
        ItemCount::Literal(count) => return Some(*count),
        ItemCount::Global(global) => {
            let form_id = resolve_property_form_id(vmad, global.property_name())?;
            world
                .try_resource::<crate::Globals>()
                .and_then(|globals| globals.get(form_id))?
        }
        ItemCount::Value(value) => evaluate_value(value, context, vmad, world, stages, deferred)?,
    };
    let count = value.trunc();
    if !(0.0..=u32::MAX as f32).contains(&count) {
        log::debug!("fragment item count skipped: {count:?} = {value}");
        return None;
    }
    Some(count as u32)
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
pub(super) fn resolve_object(
    vmad: Option<&ScriptInstanceData>,
    world: &World,
    context: QuestFormId,
    via: &ObjectRef,
    bindings: &crate::scene::SceneActorBindings,
) -> Option<byroredux_core::ecs::storage::EntityId> {
    if let ObjectRef::Linked { .. } = via {
        let target = linked_target(vmad, world, context, via, bindings)?;
        let entity = crate::condition::resolve_entity_by_global_form_id(world, target);
        if entity.is_none() {
            log::debug!("fragment effect skipped: linked ref {target:08X} has no live entity");
        }
        return entity;
    }
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

/// The global FormID an [`ObjectRef`] names — the key of the FormID-keyed
/// ledgers ([`crate::ReferenceLockState`], #4136) and the player-move
/// destination.
///
/// Direct VMAD FormID property first, alias-bound receiver second —
/// the same two-step the `Enable`/`Disable` arm uses, and for the same
/// reason: the cheap world-free path stays unchanged for the case that
/// already worked, and the alias arm is strictly additive. Returns `None`
/// when neither resolves (for a lock change the live component change still
/// stands and only its persistence is lost).
/// M47.2 — the FormID an [`ObjectRef::Linked`] names: the source
/// reference's `XLKR` link under the keyword (or the unkeyworded one), read
/// from the live source's [`crate::scene::SceneAliasCandidate`]. The target
/// itself need not be loaded.
pub(super) fn linked_target(
    vmad: Option<&ScriptInstanceData>,
    world: &World,
    context: QuestFormId,
    via: &ObjectRef,
    bindings: &crate::scene::SceneActorBindings,
) -> Option<u32> {
    let ObjectRef::Linked { from, keyword } = via else {
        return None;
    };
    let keyword = match keyword {
        Some(keyword) => resolve_property_form_id(vmad, keyword.property_name())?,
        None => 0,
    };
    let source = resolve_object(vmad, world, context, from, bindings)?;
    let target = match entity_global_form_id(world, source) {
        Some(form) => crate::linked_ref(world, form, keyword),
        None => None,
    };
    if target.is_none() {
        log::debug!("fragment effect skipped: no linked ref under keyword {keyword:08X}");
    }
    target
}

pub(super) fn object_form_id(
    vmad: Option<&ScriptInstanceData>,
    world: &World,
    context: QuestFormId,
    target: &ObjectRef,
    deferred: &DeferredFragmentEffects,
) -> Option<u32> {
    if let ObjectRef::Linked { .. } = target {
        return linked_target(vmad, world, context, target, &deferred.scene_actor_bindings);
    }
    resolve_property_form_id(vmad, target.property_name()).or_else(|| {
        let entity = resolve_object(vmad, world, context, target, &deferred.scene_actor_bindings)?;
        entity_global_form_id(world, entity)
    })
}

/// M47.2 — the reference an [`ActorRef`] names, as the FormID identity a
/// Papyrus `==` compares (`0x14` for the player); `None` for an unfilled
/// alias or a missing property — Papyrus's `None`.
pub(super) fn reference_identity(
    via: &ActorRef,
    context: QuestFormId,
    vmad: Option<&ScriptInstanceData>,
    world: &World,
    deferred: &DeferredFragmentEffects,
) -> Option<u32> {
    let player = crate::papyrus_demo::PapyrusPlayerEntity::get(world);
    let identity = |entity: EntityId| {
        if Some(entity) == player {
            Some(0x14)
        } else {
            entity_global_form_id(world, entity)
        }
    };
    match via {
        ActorRef::Player => Some(0x14),
        ActorRef::Speaker => deferred.speaker.and_then(identity),
        ActorRef::Object(object) => resolve_property_form_id(vmad, object.property_name())
            .or_else(|| {
                let entity =
                    resolve_object(vmad, world, context, object, &deferred.scene_actor_bindings)?;
                identity(entity)
            }),
    }
}

pub(super) fn resolve_actor(
    vmad: Option<&ScriptInstanceData>,
    world: &World,
    context: QuestFormId,
    via: &ActorRef,
    bindings: &crate::scene::SceneActorBindings,
    speaker: Option<EntityId>,
) -> Option<byroredux_core::ecs::storage::EntityId> {
    match via {
        ActorRef::Player => crate::papyrus_demo::PapyrusPlayerEntity::get(world),
        ActorRef::Object(via) => resolve_object(vmad, world, context, via, bindings),
        // M47.2 — the line's speaker, supplied by the dialogue caller. A
        // fragment run without one (a quest or scene fragment never lowers
        // this; a speaker-less test call) skips the effect.
        ActorRef::Speaker => {
            if speaker.is_none() {
                log::debug!("fragment effect skipped: no dialogue speaker in this run");
            }
            speaker
        }
    }
}

/// #5298 — the actor half of [`resolve_actor`] for effects whose runtime is
/// NPC-only (`StartCombat` / `StopCombat` / `SetUnconscious`). The
/// lowering-time check (`actor == ActorRef::Player`) is syntactic: Skyrim's
/// ubiquitous `Actor Property PlayerRef Auto` lowers as
/// `ActorRef::Object(..)` (VMAD `Object { form_id: 0x14, alias: -1 }`), and
/// since #4694 that resolves to the player entity at runtime — so the static
/// check alone let `PlayerRef.SetUnconscious(true)` clear the player's
/// `restrained` flag mid-cinematic and `PlayerRef.StartCombat` steer the
/// player's Transform (#4323's impact). Declining on the *resolved* identity
/// here catches the property spelling, a player-filled quest alias, and
/// `Game.GetPlayer()` alike. The player stays legal as a combat **target**,
/// which is a different argument of a different arm.
pub(super) fn resolve_npc_actor(
    vmad: Option<&ScriptInstanceData>,
    world: &World,
    context: QuestFormId,
    via: &ActorRef,
    bindings: &crate::scene::SceneActorBindings,
    speaker: Option<EntityId>,
) -> Option<byroredux_core::ecs::storage::EntityId> {
    let entity = resolve_actor(vmad, world, context, via, bindings, speaker)?;
    let player = crate::papyrus_demo::PapyrusPlayerEntity::get(world);
    if player == Some(entity) {
        log::debug!(
            "fragment effect skipped: the resolved actor is the player, whose \
             combat/unconscious runtime is unmodeled (#5298)"
        );
        return None;
    }
    Some(entity)
}

pub(crate) fn actors_3d_loaded(
    vmad: Option<&ScriptInstanceData>,
    world: &World,
    context: QuestFormId,
    actors: &[ActorRef],
    bindings: &crate::scene::SceneActorBindings,
) -> bool {
    actors.iter().all(|actor| {
        // Latent waits never carry the speaker (the lowerer declines a
        // speaker fragment with a latent tail), so none is needed here.
        resolve_actor(vmad, world, context, actor, bindings, None)
            .is_some_and(|entity| world.has::<Transform>(entity))
    })
}

pub(super) fn update_actor_cinematic_state(
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

pub(super) fn exit_cart_idle_property(seat: u8) -> Option<&'static str> {
    match seat {
        1 => Some("IdleCartPassengerAExit"),
        2 => Some("IdleCartPassengerBExit"),
        3 => Some("IdleCartPassengerDExit"),
        4 => Some("IdleCartPassengerCExit"),
        5 => Some("IdleCartDriverExit"),
        _ => None,
    }
}
