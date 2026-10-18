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
        Effect::MoveTo {
            moved,
            destination,
            offset,
            match_rotation,
        } => {
            let moved_entity =
                resolve_object(vmad, world, context, moved, &deferred.scene_actor_bindings)?;
            let destination_entity = resolve_object(
                vmad,
                world,
                context,
                destination,
                &deferred.scene_actor_bindings,
            )?;
            apply_move_to_effect(
                world,
                moved_entity,
                destination_entity,
                *offset,
                *match_rotation,
                context.0,
            );
            None
        }
        Effect::MoveToEditorLocation { moved } => {
            let moved_entity =
                resolve_object(vmad, world, context, moved, &deferred.scene_actor_bindings)?;
            let Some(placed) = world.get::<crate::EditorLocation>(moved_entity).map(|placed| *placed) else {
                log::debug!("fragment MoveToMyEditorLocation skipped: the reference has no editor location");
                return None;
            };
            let mut transforms = world.query_mut::<Transform>()?;
            let transform = transforms.get_mut(moved_entity)?;
            log::debug!(
                "fragment MoveToMyEditorLocation (quest {:08X}): {moved_entity:?} -> {:?}",
                context.0,
                placed.translation
            );
            transform.translation = placed.translation;
            transform.rotation = placed.rotation;
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
            deferred.reference_enable_changes.push((form_id, enabled));
            None
        }
        _ => unreachable!("apply_placement_effect received a non-placement effect"),
    }
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
