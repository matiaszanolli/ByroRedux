**Source:** FNV compatibility audit — Dimension 7 (PHYSAL Ragdoll), `docs/audits/AUDIT_FNV_2026-07-13.md`
**Severity:** LOW (informational — documented approximation, undocumented at the code site) · **Status when filed:** NEW, CONFIRMED

## Description
`ImportedJointKind::Ragdoll` carries `plane_min` / `plane_max` (the asymmetric swing limits decoded in `crates/nif/src/import/collision/ragdoll.rs::ragdoll_joint`), but `joint_from_imported` (`byroredux/src/ragdoll.rs:158-198`) captures them into the `..` rest pattern and maps only `cone_max` / `twist_min` / `twist_max` into `RagdollJointSpec::Ragdoll`. `build_joint` (`crates/physics/src/ragdoll.rs:315-321`) then applies a **symmetric** `[-cone, cone]` on both swing axes.

This is the `docs/engine/physal.md` §Known-approximation "cone→both swing axes" simplification — intentional — but unlike the sibling edge-drop sites (#1539 / #1718 / #1850, which `log::warn!`), this per-field loss is silent and uncommented at `joint_from_imported`, so a future reader cannot tell the plane asymmetry is deliberately discarded.

## Impact
FNV ragdoll swing cones are symmetric where the authored data was asymmetric — a fidelity gap, not a break. No runtime error.

## Suggested Fix
Add a one-line comment at `joint_from_imported` pointing at `docs/engine/physal.md` §Known-approximation and the symmetric-swing application site, so the drop is traceable. (Full fix — mapping `plane_min`/`plane_max` onto Rapier's AngY/AngZ ranges — is the planned PHYSAL rollout step 3.)

## Note on labeling
Filed under `animation` — the repo has no `physics`/`ragdoll` domain label. Type `documentation` because the actionable fix here is the traceability comment; the behavioral fix is a separately planned rollout step.

## Completeness Checks
- [ ] **SIBLING**: match the warn/comment convention used at the sibling drop sites (#1539/#1718/#1850)
