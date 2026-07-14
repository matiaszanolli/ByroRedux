**Source:** FNV compatibility audit — Dimension 7 (PHYSAL Ragdoll), `docs/audits/AUDIT_FNV_2026-07-13.md`
**Severity:** LOW · **Status when filed:** NEW, CONFIRMED against current code

## Description
`ragdoll_writeback_system` (`byroredux/src/ragdoll.rs:324-377`) moves bone `GlobalTransform`s but never the skinned-mesh root entity. `make_world_bound_propagation_system` (`byroredux/src/systems/bounds.rs`) derives leaf `WorldBound`s from `LocalBound × GlobalTransform` on the mesh entity (whose root global is untouched by ragdoll) and folds children up; ragdoll bones are pure nodes with no `LocalBound`, so the mesh's bounding volume stays anchored at the bind-pose root. A ragdoll that crumples in place stays within the bind-pose radius (benign), but one that slides/falls away from its origin can be frustum-culled or get a stale TLAS-instance bound while still on-screen.

Does **not** violate the documented Late-write / `LocalBound` invariant (`byroredux/src/boot.rs:691-699`) because ragdoll bones carry no `LocalBound`.

## Impact
Occasional cull / RT-bound pop for a ragdoll that travels far from spawn. Minor for the in-place crumple of PHYSAL slice 1.

## Suggested Fix
When any `RagdollActive` actor's simulated bodies leave the mesh's bind-pose radius, expand/recenter the mesh `WorldBound` from the live bone globals (or mark it dirty for re-derivation).

## Note on labeling
Filed under `animation` — the repo has no `physics`/`ragdoll` domain label. Root cause is ragdoll writeback; the observable is a renderer culling-bound pop.

## Completeness Checks
- [ ] **SIBLING**: the recenter path covers both frustum culling and the TLAS-instance bound (both read the mesh `WorldBound`)
- [ ] **TESTS**: a regression test moves a ragdoll body outside the bind-pose radius and asserts the mesh `WorldBound` grows/recenters to keep it enclosed
