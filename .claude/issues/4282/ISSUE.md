# SF-2026-09-11-D8-01: Starfield BSLightingShaderProperty.wetness and .luminance are parsed but have no ImportedMaterial sink — never cross the NIFAL boundary

**Issue**: #4282 — https://github.com/matiaszanolli/ByroRedux/issues/4282
**Labels**: medium,nifal,nif-parser,game:starfield,legacy-compat,bug

**Severity**: MEDIUM
**Dimension**: Dimension 8 — NIFAL Canonical Material Translation for Starfield
**Location**: `crates/nif/src/blocks/properties.rs (or shader.rs) — BSLightingShaderProperty wetness/luminance fields; crates/nif/src/import/material/ (no forwarding site)`
**Status**: NEW

## Description
Starfield's `BSLightingShaderProperty.wetness` and `.luminance` scalars are parsed from the NIF block but have no corresponding field on `ImportedMaterial` to be written into — they are read and then discarded, never crossing the NIFAL canonical-translation boundary (`translate_material`) into `Material`.

## Evidence
Verified during this audit: the two fields are present on the parsed block struct with no forwarding call anywhere in `crates/nif/src/import/material/`. Live loss is measured as small today: 97.9% of Starfield meshes are "stub-shaped" material references (per D8-03's related finding) where this doesn't matter, and the luminance quad specifically is 100% authored-default in the sampled corpus — but the structural gap (no sink exists at all) is unrecorded and would silently discard authored data the moment either field is meaningfully authored on new or DLC content.

## Impact
No measured live visual loss today (values are at their defaults in the sampled corpus), but a structural capability gap: any future Starfield content authoring non-default wetness/luminance has no path to reach the renderer through the canonical Material.

## Related
None filed.

## Suggested Fix
Add `wetness`/`luminance` fields to `ImportedMaterial` and wire them through `translate_material` into `Material`, per NIFAL's single-boundary invariant (parse-time capture, translate once, no render-time re-derivation).

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed by audit-publish from docs/audits/AUDIT_STARFIELD_2026-09-11.md — findings verified against live code during this publish run.*
