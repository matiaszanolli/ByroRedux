# #2703: FO4-D7-01: `GpuMaterial::ior`'s doc asserts FO4 BGSM v9+ authors an index of refraction - the BGSM parser has no such field and the FO4 path never writes `ior`

- **Severity**: LOW
- **Dimension**: 7 — NIFAL / GPU material contract
- **Location**: `crates/renderer/src/vulkan/material.rs:232-242`
- **Status**: NEW
- **Description**: The field doc states *"FO4 BGSM v9+ and Starfield .mat materials author this explicitly."* `crates/bgsm/src/bgsm.rs:18-140` decodes no IOR / index-of-refraction / refraction-power field at any version — the v≥9 addition is `custom_porosity` + `porosity_value`. `merge_external_material` never assigns `ior`, so every FO4 material takes `material_optical_scalar(...)` at `byroredux/src/material_translate.rs:213` (the generic dielectric default, or the glass promotion). The claim is true only for Starfield `.mat`.
- **Impact**: No runtime effect, but it is a false provenance claim inside the file that defines the GPU material layout contract — the same doc-trust failure mode as the `GpuMaterial` 300 B → 348 B drift.
- **Related**: #2415, #2273.
- **Suggested Fix**: Drop the FO4 clause or replace it with an explicit "FO4 BGSM authors no IOR; the FO4 path always takes the dielectric default."

---
**Source**: `docs/audits/AUDIT_FO4_2026-08-12.md` (finding `FO4-D7-01`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs`, per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix

