//! The placement effect applier. (#5468, split from `effects.rs`)

use super::*;

/// `MoveTo` / `Disable` / `Enable` — placement and reference-enable state.
///
/// #4340 — split out of [`apply_effect`], whose arms are now one-line
/// delegates. The bodies moved verbatim; every lock they take stays
/// named in `apply_effect`'s nested-lock residual list, which the #3949
/// scan now reads across these helpers too.
pub(super) fn apply_placement_effect(
    effect: &Effect,
    context: QuestFormId,
    vmad: Option<&ScriptInstanceData>,
    world: &World,
    deferred: &mut DeferredFragmentEffects,
) -> Option<QuestStageAdvanced> {
    match effect {
        Effect::MoveToNode { moved, target, node } => {
            let body = |entity: EntityId| !crate::is_logical_stub(world, entity);
            let moved_entity =
                resolve_object(vmad, world, context, moved, &deferred.scene_actor_bindings).filter(|e| body(*e));
            let target_entity =
                resolve_object(vmad, world, context, target, &deferred.scene_actor_bindings).filter(|e| body(*e));
            let (Some(moved_entity), Some(target_entity)) = (moved_entity, target_entity) else {
                log::debug!("fragment MoveToNode skipped: a reference has no loaded body");
                return None;
            };
            match named_descendant(world, target_entity, node) {
                Some(node_entity) => {
                    apply_move_to_effect(world, moved_entity, node_entity, [0.0; 3], true, context.0);
                }
                None => log::debug!("fragment MoveToNode skipped: no node {node} in the target's 3D"),
            }
            None
        }
        Effect::SetScale { object, scale } => {
            let entity = resolve_object(vmad, world, context, object, &deferred.scene_actor_bindings)?;
            match world.query_mut::<Transform>().as_mut().and_then(|transforms| transforms.get_mut(entity)) {
                Some(transform) => transform.scale = *scale,
                None => log::debug!("fragment SetScale skipped: the reference has no Transform"),
            }
            None
        }
        Effect::MoveTo {
            moved,
            destination,
            offset,
            match_rotation,
        } => {
            let body = |entity: EntityId| !crate::is_logical_stub(world, entity);
            let moved_entity =
                resolve_object(vmad, world, context, moved, &deferred.scene_actor_bindings).filter(|e| body(*e));
            let destination_entity =
                resolve_object(vmad, world, context, destination, &deferred.scene_actor_bindings).filter(|e| body(*e));
            if let (Some(moved_entity), Some(destination_entity)) = (moved_entity, destination_entity) {
                apply_move_to_effect(world, moved_entity, destination_entity, *offset, *match_rotation, context.0);
                return None;
            }
            // XCT-LOC-004 — an end with no body (its cell is not loaded): the
            // engine reads the target's placement and moves the actor through
            // the relocation ledger, across spaces when it must.
            let actor = object_form_id(vmad, world, context, moved, deferred)?;
            let target = object_form_id(vmad, world, context, destination, deferred)?;
            // #6916 — the player (a `PlayerRef` property, or an alias holding the
            // player: both resolve to `0x14`) moves by a cell transition, as
            // `MovePlayerTo` does. The actor ledger would evict the player's body.
            // The same terms as the lowering's `MovePlayerTo`: no offset, rotation
            // matched.
            if actor == crate::PLAYER_REFERENCE {
                if *offset != [0.0; 3] || !*match_rotation {
                    log::debug!("fragment MoveTo (player) skipped: a cell transition carries no offset ({target:08X})");
                    return None;
                }
                if let Some(mut request) = world.try_resource_mut::<super::PlayerMoveRequest>() {
                    request.0 = Some(target);
                }
                return None;
            }
            if !crate::queue_actor_move(world, crate::ActorMoveRequest::ToReference { actor, target, offset: *offset }) {
                log::debug!("fragment MoveTo skipped: no actor-move queue is installed ({actor:08X} -> {target:08X})");
            }
            None
        }
        Effect::MoveToPackageLocation { moved } => {
            let actor = object_form_id(vmad, world, context, moved, deferred)?;
            if !crate::queue_actor_move(world, crate::ActorMoveRequest::ToPackageLocation { actor }) {
                log::debug!("fragment MoveToPackageLocation skipped: no actor-move queue is installed ({actor:08X})");
            }
            None
        }
        Effect::MoveToEditorLocation { moved } => {
            let moved_entity =
                resolve_object(vmad, world, context, moved, &deferred.scene_actor_bindings)?;
            if !move_to_editor_location(world, moved_entity) {
                log::debug!("fragment MoveToMyEditorLocation skipped: the reference has no editor location");
            }
            None
        }
        Effect::MovePlayerTo { destination } => {
            let Some(form_id) = object_form_id(vmad, world, context, destination, deferred)
            else {
                log::debug!("fragment MoveTo (player) skipped: destination does not resolve");
                return None;
            };
            if let Some(mut request) = world.try_resource_mut::<super::PlayerMoveRequest>() {
                request.0 = Some(form_id);
            }
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
            // hence the trip back through `entity_global_form_id` —
            // [`object_form_id`], which also reads an unloaded linked ref.
            let form_id = object_form_id(vmad, world, context, object, deferred)?;
            // #3489 — the symmetric counterpart to Disable, sharing this arm
            // (same receiver resolution, same sink) rather than duplicating
            // it: `Effect::Enable` is the only other variant reaching here,
            // so this is exhaustive over the pattern above, not a guess.
            let enabled = matches!(effect, Effect::Enable { .. });
            deferred.reference_enable_changes.push((form_id, Some(enabled)));
            None
        }
        _ => unreachable!("apply_placement_effect received a non-placement effect"),
    }
}

pub(crate) fn move_to_editor_location(world: &World, entity: EntityId) -> bool {
    let Some(authored) = world.get::<crate::EditorLocation>(entity).map(|location| *location) else {
        return false;
    };
    let Some(mut transforms) = world.query_mut::<Transform>() else {
        return false;
    };
    let Some(transform) = transforms.get_mut(entity) else {
        return false;
    };
    transform.translation = authored.translation;
    transform.rotation = authored.rotation;
    true
}

/// The entity under `root` (its own subtree, breadth first) whose `Name` is `node`, ignoring case — a
/// node of a reference's loaded 3D.
pub(crate) fn named_descendant(world: &World, root: EntityId, node: &str) -> Option<EntityId> {
    use byroredux_core::ecs::components::{Children, Name};
    let pool = world.try_resource::<byroredux_core::string::StringPool>()?;
    let names = world.query::<Name>()?;
    let children = world.query::<Children>()?;
    let mut queue = std::collections::VecDeque::from([root]);
    let mut visited = 0usize;
    while let Some(entity) = queue.pop_front() {
        visited += 1;
        if visited > 4096 {
            return None;
        }
        if entity != root
            && names.get(entity).and_then(|name| pool.resolve(name.0)).is_some_and(|name| name.eq_ignore_ascii_case(node))
        {
            return Some(entity);
        }
        if let Some(kids) = children.get(entity) {
            queue.extend(kids.0.iter().copied());
        }
    }
    None
}

/// Place `moved` at `destination`'s position plus `offset` (Gamebryo Z-up
/// world axes), and at its rotation too when `match_rotation`. Shared by the
/// Papyrus `MoveTo` effect and the ObScript `MoveTo` command
/// ([`crate::obscript_actions`]); named `apply_*_effect` so the #3949
/// nested-lock scan reads its acquisitions with its siblings'. Returns
/// whether the entity moved. `origin` is the quest or script FormID, for the
/// log only.
pub(crate) fn apply_move_to_effect(
    world: &World,
    moved_entity: EntityId,
    destination_entity: EntityId,
    offset: [f32; 3],
    match_rotation: bool,
    origin: u32,
) -> bool {
    let Some((translation, rotation)) = world
        .get::<GlobalTransform>(destination_entity)
        .map(|gt| (gt.translation, gt.rotation))
    else {
        log::debug!("MoveTo skipped: destination entity has no GlobalTransform");
        return false;
    };
    // The offset is in Gamebryo's Z-up world axes.
    let translation = translation + byroredux_core::math::Vec3::from(byroredux_core::math::coord::zup_to_yup_pos(offset));
    let Some(mut transforms) = world.query_mut::<Transform>() else {
        log::debug!("MoveTo skipped: Transform component never registered");
        return false;
    };
    // Unlike AddItem, a "moved" entity with no Transform isn't a
    // container the effect can adopt — it isn't a placed spatial
    // entity at all, so this declines rather than fabricating one.
    let Some(transform) = transforms.get_mut(moved_entity) else {
        log::debug!("MoveTo skipped: moved entity has no Transform");
        return false;
    };
    log::debug!(
        "MoveTo ({origin:08X}): {moved_entity:?} -> {destination_entity:?} at {translation:?}"
    );
    transform.translation = translation;
    if match_rotation {
        transform.rotation = rotation;
    }
    true
}
