# 3262: CONC-D3-2026-08-24-03: wander_system_inner and patrol_system_inner skipped #2134's snapshot-before-PhysicsWorld restructure

**Severity**: MEDIUM · **Report**: `docs/audits/AUDIT_CONCURRENCY_2026-08-24.md` (CONC-D3-2026-08-24-03)

## Description

#2134 restructured `follow`/`escort`/`travel`/`guard` into a snapshot-then-physics two-pass shape. `patrol.rs`/`wander.rs` were never restructured — both still acquire `PhysicsWorld` inside a block holding five storage read guards (`Transform`, `PatrolState`/`WanderState`, `NavmeshTile`, `NavPath`, behavior).

## Location

`byroredux/src/systems/patrol.rs:80-93`; `byroredux/src/systems/wander.rs:243-259`

## Trigger Conditions

Any cell with a `WanderBehavior`/`PatrolBehavior` actor and a live `PhysicsWorld`. Both systems are `Stage::PostUpdate` exclusives today, so no live hang — promoting either to parallel removes that protection.

## Impact

Not a live cycle today (no reverse `PhysicsWorld → storage` edge exists). Two systems sit outside the convention the other four now encode, with a five-guard hold window a future reversed site would immediately close a cycle against.

## Related

#2134, #2404, #3130.

## Suggested Fix

Apply the sibling shape — hoist the per-entity snapshot under the storage guards, close the block, run the physics-touching step in a second block, mechanically identical to `follow.rs:246-267`.

## Completeness Checks
- [ ] **LOCK_ORDER**: Matches the #2134 sibling shape
- [ ] **SIBLING**: `follow.rs`/`escort.rs`/`travel.rs`/`guard.rs` already correct — mirror exactly
