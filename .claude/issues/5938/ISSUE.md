# PHYS-D3-2026-10-16-01: The XRGD corpse-pose wiring (spawn parking, consume-before-activate, one-shot) has no test

Labels: low,physics,test-gap,bug,game:skyrim
Source: docs/audits/AUDIT_PHYSICS_2026-10-16.md

- **Severity**: LOW
- **Dimension**: Ragdoll & Constraint Seam
- **Location**: `byroredux/src/combat.rs:1040-1063` (reconcile wiring), `byroredux/src/cell_loader/references/mod.rs:765-785` (parking), tests in `byroredux/src/ragdoll_pose_tests.rs`
- **Status**: NEW (feature itself tracked by Existing #5015, still open for FO4/creatures)
- **Description**: `ragdoll_pose_tests.rs` covers `bone_poses`, `body_entries`, `is_humanoid_ragdoll` and `pose_skeleton` in isolation, plus an ignored corpus-count test. Nothing exercises the two call sites that make the feature work: (a) the spawn-side condition `placed_ref.starts_dead && !placed_ref.ragdoll_pose.is_empty()` that inserts `AuthoredRagdollPose`, and (b) `reconcile_dead_actor` taking the component, calling `pose_skeleton` strictly BEFORE `activate_ragdoll`, consuming it whether or not it applies, and surfacing the "posed N bones from XRGD" / "XRGD pose not applied" note. `grep AuthoredRagdollPose byroredux/src` hits only `combat.rs`, `ragdoll_pose.rs`, `boot/world.rs` (register), the spawn site, and the save-registry exclusion list.
- **Evidence**: `rg -l AuthoredRagdollPose byroredux/src` -> combat.rs, ragdoll_pose.rs, boot/world.rs, registry_completeness_tests.rs, cell_loader/references/mod.rs; no `#[test]` in combat.rs or the loader references the component. Swapping the order of the two calls in `reconcile_dead_actor` (seed then pose), or dropping the `remove_component_taking`, would keep every current test green.
- **Impact**: Code works today (the commit message reports a live RannveigsFast01 check). The risk is regression: the ordering is the whole point (the seed validator and `activate_ragdoll` read the posed globals) and is currently pinned only by prose.
- **Trigger Conditions**: A future refactor of `reconcile_dead_actor` or of the starts-dead branch in `load_references_budgeted`.
- **Related**: #5015, `registry_completeness_tests.rs` (documents the non-saved status)
- **Suggested Fix**: Add one headless test using the `humanoid_chain` fixture: insert `AuthoredRagdollPose` + `Dead` on the root, run `reconcile_dead_actor`, assert the COM `Transform.translation` is zero, the component is gone, and the returned note starts with "; posed 18 bones" (or "XRGD pose not applied" for a renamed body); plus a source-order guard that `pose_skeleton(` precedes `activate_ragdoll(` inside `reconcile_dead_actor`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / the sibling producer or consumer
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix

_Filed from `docs/audits/AUDIT_PHYSICS_2026-10-16.md`. Fix with `/fix-issue <number>`._
