# PHYS-D1-2026-08-16-02: ragdoll limb colliders are scaled twice

**Issue**: #3065
**Severity**: HIGH
**Labels**: `high,legacy-compat,bug`
**Source report**: `docs/audits/AUDIT_PHYSICS_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_PHYSICS_2026-08-16.md` (Dimension 1 — collider construction).

**Location**: `byroredux/src/ragdoll.rs`:314 (first application) · `crates/physics/src/convert.rs` (second)

## Description

Ragdoll limb colliders are **scaled twice** — a scaled actor's rig is `scale²` geometry on `scale¹` articulation.

Same root cause as #3064, but with an extra failure mode: the *articulation* (joint frames, limits) is scaled once while the *geometry* is scaled twice, so the two disagree.

## Evidence

`ragdoll.rs`:314 applies scale before handing shapes to the converter; `crates/physics/src/convert.rs` applies `* scale` again on every shape variant (:175, :180, :213).

Re-verified 2026-08-17.

## Impact

A scaled actor (children, super mutants, any `XSCL`-scaled NPC) gets limb colliders `scale²` while its joints sit at `scale¹` positions. The rig is internally inconsistent: limbs overlap or float relative to their constraints, which a solver resolves as explosive separation or jitter.

This is the PHYSAL layer's core geometry contract, and it is wrong for every non-unit-scale actor.

## Suggested Fix

Remove the first application in `ragdoll.rs` and let `convert.rs` own scaling, as in #3064 — then verify joint frames and limits are scaled consistently with the geometry.

## Related

- #3064 (PHYS-01 — the same double-scale in the static-trimesh path; one fix should cover both)
- `docs/engine/physal.md` (the layer spec this contract belongs to)

## Completeness Checks
- [ ] **SINGLE-SITE**: One scale owner, shared with #3064's fix
- [ ] **ARTICULATION-PARITY**: Joint frames and limits scale consistently with limb geometry
- [ ] **SIBLING**: All `build_ragdoll` shape paths covered, not just the limb capsules
- [ ] **TESTS**: A regression test builds a scaled ragdoll and asserts geometry/articulation agreement

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3065 --json state` when live state is needed.*
