# NIFAL-D2-03: SkinnedMesh.bones: Vec<Option<EntityId>> is an unresolved-reference sentinel nifal.md's 'skinning — converged' entry doesn't record

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2441
**Finding ID**: NIFAL-D2-03 (source: `docs/audits/AUDIT_LEGACY_COMPAT_2026-08-07.md`)

**Severity**: LOW
**Dimension**: 2 — NIFAL mapping shape
**Location**: `crates/core/src/ecs/components/skinned_mesh.rs:63-69,184-201`; producer `byroredux/src/scene/nif_loader.rs:966-1024`
**Status**: NEW

## Description
`bones`/`skeleton_root` carry `Option`s past the boundary; `compute_palette_into` substitutes identity for `None`. Thoroughly documented and logged at the component/producer level (not a silent leak) — but nifal.md §2 marks skinning "converged" with no residual note.

## Impact
Documentation-shape only. The concrete risk is a future audit reading "converged" and skipping the check — the exact #2206 failure mode.

## Suggested Fix
Add a one-line residual note to nifal.md §2 recording the `Option` as a terminal "bone-name lookup failed" state, not a resolve-later leak.

## Completeness Checks
- [ ] **TESTS**: N/A (doc-only change)
