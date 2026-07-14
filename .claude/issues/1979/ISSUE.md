**Source:** FNV compatibility audit — Dimension 7 (PHYSAL Ragdoll), `docs/audits/AUDIT_FNV_2026-07-13.md`
**Severity:** MEDIUM · **Status when filed:** NEW, CONFIRMED against current code

## Description
`ragdoll_writeback_system` (`byroredux/src/ragdoll.rs:324`, Stage::Late) overwrites `GlobalTransform` **only** on the bones that are ragdoll bodies (the 18 FNV ragdoll bones). It runs *after* `make_transform_propagation_system` (PostUpdate) has already recomputed **every** bone's `GlobalTransform` from its still-animated local `Transform`. Critically, `animation_system` is **not** gated on `RagdollActive` — verified: there is no `RagdollActive`/`Ragdoll` reference anywhere in `byroredux/src/systems/animation.rs` — so it keeps sampling clips and writing bone-local `Transform`s every frame on the ragdolled actor.

Consequence: any bone that is a **descendant of a ragdoll body but is not itself a ragdoll body** — on the vanilla FNV skeleton, the finger bones (children of `Bip01 [LR] Hand`) and the toe bones — has its `GlobalTransform` computed as `animated_parent_global × child_local` during PostUpdate and is **never** re-derived from the simulated parent pose (writeback touches neither those children nor the parent's local `Transform`). The animated skeleton stays anchored at the spawn/standing pose while the ragdoll bodies crumple, so the fingers/toes float in the air at the pre-ragdoll pose, visibly detached from the crumpled hands/feet.

## Evidence
- `ragdoll_writeback_system` writes `gt.rotation` / `gt.translation` for `ragdoll.bodies` only — no children walk, no local `Transform` update.
- Stage order in `byroredux/src/boot.rs`: Update (`animation_system`, ungated) → PostUpdate (transform propagation) → Late (ragdoll writeback).
- The scene graph is left internally inconsistent for ragdoll bones (`global ≠ parent_global × local`), so any consumer reading a ragdoll bone's *local* transform, or a *child's* global, sees the animated pose.
- The skin path reads bone globals directly (`byroredux/src/render/skinned.rs:174-181`, `gt.to_matrix()`), which is why the ragdoll bones render correctly but their non-simulated children do not.

## Impact
Visible on every ragdoll activation (`ragdoll <id>` console path / `m41-ragdoll.sh` smoke): fingers and toes hang detached at the standing pose. Debug-triggered feature today (not yet wired to gameplay death), so blast radius is the reference demo — but it is the exact output the PHYSAL reference slice exists to showcase.

## Related
- `docs/engine/physal.md` §3 documents the *ragdoll-bone* overwrite but not the *descendant* case; §Known-approximation covers only joint-limit fidelity.
- Cross-checked against Dimension 6 skin path (no palette corruption — the written values are valid matrices; this is a *stale-but-valid* pose, not a corrupt palette).

## Suggested Fix
Gate `animation_system` off for `RagdollActive` actors (skip clip sampling on their subtrees), and after writeback either:
1. re-run a localized transform propagation over the ragdoll subtree so non-simulated descendants inherit the simulated parent global, **or**
2. write each ragdoll bone's *local* `Transform` (relative to its now-simulated parent) so the standard PostUpdate propagation next frame carries the pose down to the fingers/toes.

## Completeness Checks
- [ ] **SIBLING**: the fix covers the full ragdoll subtree, not just the direct body bones (fingers, toes, and any other non-body descendants)
- [ ] **TESTS**: a regression test asserts a ragdoll bone's non-simulated child global tracks the simulated parent after writeback
