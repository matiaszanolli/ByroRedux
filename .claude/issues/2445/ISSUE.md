# MAT-D3-03: The 'cannot diverge' claim on the normal-alpha-as-spec pair is false for Material-less entities

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2445
**Finding ID**: MAT-D3-03 (source: `docs/audits/AUDIT_LEGACY_COMPAT_2026-08-07.md`)

**Severity**: LOW
**Dimension**: 3 — Material translation boundary (NIFAL reference slice)
**Location**: `render/static_meshes.rs:405-427`; `material_translate.rs:249-268,334-347`
**Status**: NEW

## Description
A comment asserts the per-draw gloss-slot binding and the spawn-side scalar resolve "cannot diverge" because they share a predicate — true, but the spawn write-back silently no-ops on any entity without a `Material` (early-return), while the render-side gate has no such guard and substitutes fallback values that still pass the gate.

## Impact
Behaviourally small (still darkens rather than brightens), but the asserted invariant isn't enforced — exactly the trap a future `Material`-less population (MAT-D3-02) would fall into.

## Related
MAT-D3-02 (this report — the concrete instance of `Material`-less exterior draws that would trigger this gap).

## Suggested Fix
Fix MAT-D3-02 (removes all `Material`-less draws), or gate the render-side binding on `mat.is_some()`.

## Completeness Checks
- [ ] **TESTS**: A regression test exercises a `Material`-less entity through the render-side gate and confirms it doesn't silently pass
