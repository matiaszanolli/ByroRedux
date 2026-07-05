**Severity**: LOW (documentation) · **Dimension**: FO3 Rendering — Inline Shaders · **Game**: FO3 (bsver 34), also FNV (shared 102 path)
**Source**: `docs/audits/AUDIT_FO3_2026-07-05.md` (FO3-D1-001)

## Description
The comment above the `MATERIAL_KIND_NO_LIGHTING` early-return in `crates/renderer/shaders/triangle.frag` states the alpha-test discard "already ran (~line 1100)". The alpha-test discards (`if (!pass) discard;` and the implicit fully-transparent discard) actually run well *above* the NO_LIGHTING return; the cited "~line 1100" points *past* the return, inverting the apparent order. Behavior is correct — this is a stale line reference.

## Evidence
- `crates/renderer/shaders/triangle.frag` — the `mat.materialKind == MATERIAL_KIND_NO_LIGHTING` block; the "(~line 1100)" note sits above the return, but the per-instance alpha-test `discard` is upstream of it.
- `MATERIAL_KIND_NO_LIGHTING = 102u` in `crates/renderer/shaders/include/shader_constants.glsl`; the 102 tag is set in `crates/nif/src/import/material/walker.rs` (`BSShaderNoLightingProperty` arm).

## Impact
None at runtime. Maintenance risk only: a future edit to the FO3 terminal/HUD/neon/blood-splat NO_LIGHTING path could "reinstate" an alpha-test discard believing it was skipped, double-discarding or reordering state.

## Suggested Fix
Replace "(~line 1100)" with a symbol reference (e.g. "the per-instance alpha-test discard above") so the note survives line drift. Documentation-only; no behavioral change, no cargo-test visibility.

## Completeness Checks
- [ ] **SIBLING**: Same stale line-number pattern checked in nearby `triangle.frag` comments
- [ ] **TESTS**: N/A — comment-only change, no behavioral surface to pin
