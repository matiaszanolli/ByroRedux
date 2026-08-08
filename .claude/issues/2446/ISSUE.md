# MAT-D3-04: Four comments describe the decal-layer escalation with the wrong input field and the wrong output layer

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2446
**Finding ID**: MAT-D3-04 (source: `docs/audits/AUDIT_LEGACY_COMPAT_2026-08-07.md`)

**Severity**: LOW
**Dimension**: 3 — Material translation boundary (NIFAL reference slice)
**Location**: `cell_loader/spawn.rs:405,1585-1591`; `scene/nif_loader.rs:848-852`; `render/static_meshes.rs:119`
**Status**: NEW

## Description
Comments at four sites describe decal-layer escalation via `alpha_test_func != 0` → `RenderLayer::Decal`. The actual implementation (`render_layer_with_decal_escalation`, `crates/core/src/ecs/components/render_layer.rs:123-134`) uses the `alpha_test: bool` field (not `alpha_test_func`, which is never read at these sites — its own doc comment explains why: the Gamebryo default for `alpha_test_func` is `6`/GREATEREQUAL and would over-escalate every architectural mesh) and escalates cutout architecture to `RenderLayer::Clutter`, not `Decal` — only `mesh_is_decal` yields `Decal`.

## Impact
Documentation only, no runtime effect — but misdescribes a depth-bias rule at four sites including the render hot path.

## Suggested Fix
Correct all four comments to match the actual `is_decal`/Clutter behavior documented at `render_layer_with_decal_escalation`'s own doc comment.

## Completeness Checks
- [ ] **TESTS**: N/A (comment-only change)
