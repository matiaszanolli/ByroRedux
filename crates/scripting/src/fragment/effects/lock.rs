//! The lock effect applier. (#5468, split from `effects.rs`)

use super::*;

/// `SetLocked` / `SetLockLevel` — the `Locked` component and the persistent
/// lock ledger (#3159 / #4136 / #4329 / #4330).
///
/// #4340 — split out of [`apply_effect`], whose arms are now one-line
/// delegates. The bodies moved verbatim; every lock they take stays
/// named in `apply_effect`'s nested-lock residual list, which the #3949
/// scan now reads across these helpers too.
pub(super) fn apply_lock_effect(
    effect: &Effect,
    context: QuestFormId,
    vmad: Option<&ScriptInstanceData>,
    world: &World,
    deferred: &mut DeferredFragmentEffects,
) -> Option<QuestStageAdvanced> {
    match effect {
        Effect::SetLocked { target, locked } => {
            // #3159 — the removal half that did not exist. `Locked` had one
            // insert (the cell loader's XLOC stamp) and one read (the
            // interaction gate) and nothing that cleared it, so an authored
            // lock was a one-way door for the session and any fragment
            // containing `Lock(false)` declined wholesale — discarding its
            // sibling stage/objective effects too.
            let Some(target_entity) =
                resolve_object(vmad, world, context, target, &deferred.scene_actor_bindings)
            else {
                // #4330 — no loaded entity. The ledger exists to outlive the
                // entity (the `ReferenceEnableState` posture, #3278), so an
                // *unlock* of a direct VMAD reference is still recorded and
                // takes effect when its cell next loads. A re-lock is not:
                // with no live component there is no authored `XLOC` level or
                // key to record, and inventing one would overwrite both.
                if !*locked {
                    if let Some(form_id) = resolve_property_form_id(vmad, target.property_name()) {
                        deferred
                            .reference_lock_changes
                            .push((form_id, DeferredLockChange::Unlocked));
                    }
                }
                return None;
            };
            // Through `query_mut`, not `World::insert`/`remove`: this system
            // holds `&World`, so structural mutation is unavailable, but
            // inserting into and removing from an *existing* storage is not
            // structural. `byroredux/src/boot/world.rs` pre-registers `Locked` so the storage is
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
            // #4136 — mirror the resulting component state into the
            // persistent ledger. Read back rather than re-deriving from
            // `*locked`: the re-lock branch above deliberately keeps
            // whatever the entity already carried, so the component is the
            // only place that knows the final level/key. Recording the
            // *outcome* is what keeps the two from drifting.
            let recorded = match locks.get(target_entity) {
                Some(l) => DeferredLockChange::Locked {
                    lock_level: l.lock_level,
                    key_form_id: l.key_form_id,
                },
                None => DeferredLockChange::Unlocked,
            };
            drop(locks);
            if let Some(form_id) = object_form_id(vmad, world, context, target, deferred) {
                deferred.reference_lock_changes.push((form_id, recorded));
            }
            None
        }
        Effect::SetLockLevel { target, level } => {
            // Difficulty only — never locks or unlocks (see the effect's
            // doc). An unlocked object has no `Locked` component to carry a
            // level, so this is a no-op there rather than an implicit lock.
            let target_entity =
                resolve_object(vmad, world, context, target, &deferred.scene_actor_bindings)?;
            let mut recorded = None;
            if let Some(mut locks) = world.query_mut::<Locked>() {
                if let Some(state) = locks.get_mut(target_entity) {
                    state.lock_level = *level;
                    // #4329 — record the whole outcome, key included, the
                    // same read-back `SetLocked` does. Queuing only the level
                    // let the ledger invent `key_form_id: None` for a
                    // reference with no prior override, so a keyed door's
                    // entry lost its key while the live component kept it —
                    // and the ledger wins over `XLOC` on the next load.
                    recorded = Some(DeferredLockChange::Locked {
                        lock_level: state.lock_level,
                        key_form_id: state.key_form_id,
                    });
                } else {
                    log::debug!(
                        "fragment SetLockLevel skipped: '{}' is not locked, so there is \
                         no lock record to set a difficulty on",
                        target.property_name()
                    );
                }
            }
            // #4136 — only record when the live component actually took the
            // change. Queuing it unconditionally would let the ledger
            // re-lock a door on the next cell load that `SetLockLevel`
            // explicitly declined to lock here.
            if let Some(recorded) = recorded {
                if let Some(form_id) = object_form_id(vmad, world, context, target, deferred) {
                    deferred.reference_lock_changes.push((form_id, recorded));
                }
            }
            None
        }
        _ => unreachable!("apply_lock_effect received a non-lock effect"),
    }
}
