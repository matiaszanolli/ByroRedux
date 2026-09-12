# SF-2026-09-11-D8-02: from_bgsm is overloaded between FO4's spec-glossiness convention and #2710's glass-promotion signal — Starfield satisfies the second meaning but reads as the first, so its effect-shader glass can never take the dielectric path

**Issue**: #4283 — https://github.com/matiaszanolli/ByroRedux/issues/4283
**Labels**: medium,nifal,game:starfield,legacy-compat,bug

**Severity**: MEDIUM
**Dimension**: Dimension 8 — NIFAL Canonical Material Translation for Starfield
**Location**: `crates/nif/src/import/types.rs:641 (from_bgsm field); byroredux/src/helpers.rs:76-101 (glass classification, keyword_match && from_bgsm); byroredux/src/asset_provider/material/cdb.rs:232-233 (from_bgsm deliberately NOT set)`
**Status**: NEW

## Description
The `from_bgsm` flag on `ImportedMaterial` carries two incompatible meanings read by two different call sites: (1) an FO4-specific signal that a BGSM/BGEM spec-glossiness convention was used (gating `BGSM_AUTHORED`/metalness-roughness translation), and (2) since #2710, a general "an external material file was successfully resolved" glass-promotion signal, read by the glass classifier in `helpers.rs:101` (`bgem_glass || (keyword_match && from_bgsm)`). Starfield's CDB-resolved materials satisfy meaning (2) — an external material description WAS resolved — but `apply_cdb_pbr_fallback` deliberately does not set `from_bgsm` (correctly, per meaning (1): the CDB isn't a BGSM spec-glossiness file), so Starfield effect-shader glass can never take the dielectric path FO4's identical authoring does through this classifier.

## Evidence
`byroredux/src/asset_provider/material/cdb.rs:232-233`: `// from_bgsm deliberately NOT set — that flag gates BGSM spec-glossiness translation (an FO4-specific format convention).` — correct for meaning (1), but this is the same flag `helpers.rs:101` reads for meaning (2). Measured during this audit: 748 Starfield effect-shader glass blocks are affected.

## Impact
Starfield effect-shader glass (748 blocks in the sampled corpus) is classified through the non-dielectric path instead of the correct glass/dielectric BSDF path — a rendering-correctness gap that becomes live the moment CDB Phase 2 (#3398) lands texture paths for these materials (today the visual difference is masked by the materials having no resolved textures yet).

## Related
Adjacent to #2710 (introduced the glass-promotion meaning of `from_bgsm`) and #3398 (CDB Phase 2, which will make this live).

## Suggested Fix
Split the two meanings onto separate fields (e.g. keep `from_bgsm` for the FO4 spec-glossiness convention, add a distinct `external_material_resolved` bool for the glass-promotion signal that `apply_cdb_pbr_fallback` DOES set), and update `helpers.rs`'s glass classifier to read the correct one.

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed by audit-publish from docs/audits/AUDIT_STARFIELD_2026-09-11.md — findings verified against live code during this publish run.*
