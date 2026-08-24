# 3261: CONC-D3-2026-08-24-02: canonical lock-acquisition order doc omits CharacterController/RapierHandles

**Severity**: MEDIUM · **Report**: `docs/audits/AUDIT_CONCURRENCY_2026-08-24.md` (CONC-D3-2026-08-24-02)

## Description

The canonical chain (`Transform → Parent → Children → GlobalTransform → SkinnedMesh → MeshHandle → LocalBound → WorldBound → Name → StringPool`) omits `byroredux_physics::CharacterController`/`RapierHandles` — exactly the types that, in `character.rs`, are acquired both before `Transform` and after `GlobalTransform` (the mechanism of #3260). `character.rs:199-203` documents an ordering constraint (#2135) purely as a local comment.

## Location

`docs/engine/ecs.md:594-612`

## Impact

The one artifact meant to make hand-ordered N-lock acquisition auditable is silent about the cluster with the most live inversions.

## Related

#3260 (CONC-D3-2026-08-24-01), #2388, #2135, #2404.

## Suggested Fix

Extend the chain with the physics pair, and hoist `character.rs:199-203`'s local note into the doc as a worked example.

## Completeness Checks
- [ ] **TESTS**: N/A — documentation-only fix
