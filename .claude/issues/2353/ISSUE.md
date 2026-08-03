# SF-D8-02: BSLightingShaderProperty material-reference stub's placeholder scalars copied unconditionally, falsely claiming authorship

**GitHub Issue**: https://github.com/matiaszanolli/ByroRedux/issues/2353
**Labels**: bug,nif-parser,medium,legacy-compat

---

**Severity**: MEDIUM
**Dimension**: 8 — NIFAL Canonical Material Translation (Starfield audit, 2026-08-03)
**Location**: `crates/nif/src/blocks/shader.rs:784-827` (`BSLightingShaderProperty::material_reference_stub`), `crates/nif/src/import/material/dedicated_shader.rs:290-351` (`apply_bs_lighting_shader`, "Capture rich material data" block)
**Status**: NEW, CONFIRMED against current code

## Description

Starfield's `BSLightingShaderProperty` material-reference stub (returned whenever the block name is non-empty — effectively all ~189,801 `BSLightingShaderProperty` blocks in the Meshes01 corpus) ships fabricated placeholder scalars: `emissive_multiple: 1.0`, `glossiness: 1.0`, `specular_color: [1,1,1]`, `specular_strength: 1.0`. `apply_bs_lighting_shader`'s "Capture rich material data" block copies these onto `MaterialInfo` **unconditionally** — it never checks `shader.material_reference`, the flag the stub itself sets for exactly this purpose — and sets `emissive_source = EmissiveSource::Lighting` plus `has_material_data = true`, both falsely claiming authorship.

## Evidence

- `shader.rs:784-827` (`material_reference_stub`): constructs the stub with `material_reference: true` and the fabricated placeholder scalars listed above.
- `dedicated_shader.rs:290-351`: `info.emissive_mult = shader.emissive_multiple; ... info.specular_color = shader.specular_color; info.specular_strength = shader.specular_strength; info.glossiness = shader.glossiness; ... info.emissive_source = EmissiveSource::Lighting; ... info.has_material_data = true;` — no `if !shader.material_reference` guard anywhere in this block.

## Impact

NIFAL no-fabrication violation, and a trap for the CDB Phase-2 work (see SF-D9-2026-08-03-03): a future implementer reading `emissive_source == Lighting` would reasonably conclude the NIF authored these values and write merge logic that defers to them, silently suppressing CDB-authored data for all ~189,801 materials. Combined with SF-D9-2026-08-03-03 (CDB `.mat` arm currently forwards zero authored data), ~189,801 of 190,549 Starfield surfaces currently reach the Disney BSDF lobe as untextured, matte, fully-dielectric white.

**Related**: SF-D8-01 (#2352) — same fabrication family (unauthored defaults treated as real), different site.

## Suggested Fix

Gate the rich-material capture block on `!shader.material_reference`, leaving `emissive_source = None` and `has_material_data = false` when the block body was never actually parsed.

## Completeness Checks
- [ ] **SIBLING**: Check whether the same unconditional-copy pattern exists for other stub/reference paths in the codebase (BGSM/BGEM external-reference paths already gate correctly per Dimension 9 — confirm this is the only ungated one)
- [ ] **CANONICAL-BOUNDARY**: Fix stays in `crates/nif/src/import/material/dedicated_shader.rs`, upstream of `translate_material`. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this — a `material_reference_stub`-sourced `BSLightingShaderProperty` must yield `has_material_data == false`
