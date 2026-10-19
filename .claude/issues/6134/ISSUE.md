# PHYS-SD-03: Ragdoll activation leaves shape-only bone entities behind, which keeps the #3477 newcomer-scan cache permanently off

Labels: low,physics,performance,bug
URL: https://github.com/matiaszanolli/ByroRedux/issues/6134

**Severity**: LOW · **Dimension**: Step & Sync (Dim 2)

## Location
`byroredux/src/ragdoll.rs` (activation removes `RapierHandles` and `RigidBodyData` from the bones but not `CollisionShape`); `crates/physics/src/sync.rs` (`collect_newcomers`: `all_registered = false` for a handle-less shape row)

## Trigger Conditions
Any ragdoll activation in the session (any killed NPC, or any Starts-Dead corpse; 1,445 Skyrim ones exist). The effect lasts while the corpse's bone entities are resident.

## Description
`collect_newcomers` caches "nothing new" in `registered_shape_generations` only when every `CollisionShape` row has a `RapierHandles` row (`all_registered`). `activate_ragdoll` strips the bones' `RapierHandles` and `RigidBodyData` but leaves their `CollisionShape` (the keyframed bone bodies are replaced by the `Ragdoll` bodies). Each bone is then an entity with a shape and neither a body nor handles, i.e. permanently "incomplete" (the test `newcomer_cache_does_not_hide_late_body_or_transform_components` pins that an incomplete row must never be cached). One corpse (~18 such bones) therefore re-enables the full `CollisionShape` storage walk on every physics tick, which #3477 was written to remove.

## Evidence
No `remove::<CollisionShape>` on the ragdoll path (`player_body.rs` and `loot_appearance.rs` are the only strippers, both for the player's visual-only parts). The `collect_newcomers` loop skips handled rows and sets `all_registered = false` otherwise (verified at publish time).

## Impact
One sparse-set probe per collider row per frame. #3477 measured ~10 us at a few thousand rows (3-5 ns per row); a radius-12 exterior with 50-100k colliders would cost roughly 0.2-0.5 ms per frame (derived, not measured). Streaming frames already invalidate the cache whenever they add or drop colliders, so this only affects steady state.

## Related
#3477 (closed; its fix works for the case it targeted), #2867.

## Suggested Fix
Have `activate_ragdoll` also remove the spec bodies' `CollisionShape` (the template keeps its own shapes), or let `collect_newcomers` skip rows that carry no `RigidBodyData` for the cache decision.

Source: docs/audits/AUDIT_PHYSICS_2026-10-19.md (area-scoped, streaming-deep)

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix
