# SF-2026-09-11-D8-04: Starfield's water-concentration unit convention is normalized in water.frag at draw time instead of at the parser→canonical translate boundary

**Issue**: #4285 — https://github.com/matiaszanolli/ByroRedux/issues/4285
**Labels**: low,water,nifal,game:starfield,legacy-compat,bug

**Severity**: LOW
**Dimension**: Dimension 8 — NIFAL Canonical Material Translation for Starfield
**Location**: `crates/renderer/shaders/water.frag (STARFIELD_WATER_CONCENTRATION_REFERENCE); NIFAL translate_material boundary`
**Status**: NEW

## Description
`water.frag` contains `STARFIELD_WATER_CONCENTRATION_REFERENCE` — a per-game unit-convention constant used to normalize Starfield's water-concentration authoring at draw time. This is the one and only per-game token found anywhere in shader source during this audit's grep sweep, and it violates the stated invariant (see the project's own "Format Translation Layer" guidance) that per-game translation belongs at the parser→`Material` boundary, never in the shader/renderer.

## Evidence
Confirmed by a shader-source grep during this audit: exactly one per-game token (`STARFIELD_WATER_CONCENTRATION_REFERENCE`) exists in any shader source in the codebase.

## Impact
Not a rendering-correctness defect today (the constant is presumably correct for Starfield content), but an architectural-invariant violation: it is the single exception to an otherwise-clean single-boundary translation design, and sets a precedent a future per-game water quirk could follow instead of going through the canonical boundary.

## Related
None filed. Cross-references the project's material-abstraction / NIFAL design guidance on keeping per-game logic out of shaders.

## Suggested Fix
Move the Starfield water-concentration normalization into `translate_material` (or the water-specific canonical translation path, WATAL), converting the authored value into a canonical unit at parse/translate time so `water.frag` needs no per-game constant.

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed by audit-publish from docs/audits/AUDIT_STARFIELD_2026-09-11.md — findings verified against live code during this publish run.*
