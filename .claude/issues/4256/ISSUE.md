# SKY-D7-2026-09-11-02: ImportedMaterial.shader_type discriminator never crosses the NIFAL boundary into canonical Material

Issue: https://github.com/matiaszanolli/ByroRedux/issues/4256

**Severity**: MEDIUM
**Dimension**: 7 — NIFAL Canonical Material Translation (Skyrim slice)
**Location**: `crates/nif/src/import/types.rs:760`, `byroredux/src/material_translate.rs:486-647`
**Status**: NEW

**Description**: Structural root cause of the companion HIGH finding (SKY-D7-2026-09-11-01): `ImportedMaterial.shader_type` never crosses the NIFAL boundary into `Material`. Only the per-variant *payload* (`shader_type_fields`) crosses; the discriminator that identifies which variant it belongs to does not, so once `material_kind` (seeded from the same raw value) is reassigned by the glass classifier, no canonical field retains the original authored provenance — forcing at least one downstream consumer (`TextureSlotContext` at cell-spawn) to read back into the raw `ImportedMaterial` tier directly, a NIFAL single-boundary violation in the making.

**Evidence**: Confirmed in current code — `ImportedMaterial.shader_type: u32` is defined at `crates/nif/src/import/types.rs:760`, but the canonical `Material` struct (`crates/core/src/ecs/components/material.rs`) has no standalone `shader_type` field — only `shader_type_fields: Option<Box<ShaderTypeFields>>` (the payload, not the discriminator) crosses via `translate_material` (`material_translate.rs:486-647`).

**Impact**: No independent runtime effect beyond enabling the companion HIGH finding — this is the structural gap that makes that overwrite unrecoverable once it happens. Also forces at least one downstream consumer to bypass the canonical boundary and read the raw `ImportedMaterial` tier directly.

**Related**: SKY-D7-2026-09-11-01 (the HIGH finding this enables); shared suggested-fix direction.

**Suggested Fix**: Add an explicit canonical `source_shader_type` field on `Material`, distinct from the engine-dispatch `material_kind`, so downstream consumers (including the glass classifier and `TextureSlotContext`) can recover the authored provenance without reading back into `ImportedMaterial`.

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: The new field is populated only at the single `translate_material` boundary, never re-derived at render time — see `/audit-nifal`.
- [ ] **SIBLING**: Audit `TextureSlotContext`'s existing read-back into `ImportedMaterial` and retarget it at the new canonical field once added
- [ ] **TESTS**: A regression test asserts `Material::source_shader_type` survives `classify_glass_into_material` unchanged even when `material_kind` is reassigned
