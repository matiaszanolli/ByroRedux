# #2704: FO4-D7-02: Eleven BGSM scalars are decoded with no `ImportedMaterial` sink and no deferral comment - including the entire wetness-control suite

- **Severity**: LOW
- **Dimension**: 7 — NIFAL boundary completeness
- **Location**: `crates/bgsm/src/bgsm.rs:68-101`; no reader in `byroredux/src/asset_provider/material.rs`
- **Status**: NEW (same class as the OPEN #2607 / #2608 / #2627 / #2642, distinct fields)
- **Description**: Repo-wide grep outside `crates/bgsm/` returns zero references for `wetness_control_spec_scale`, `wetness_control_spec_power_scale`, `wetness_control_spec_min_var`, `wetness_control_env_map_scale`, `wetness_control_fresnel_power`, `wetness_control_metalness`, `custom_porosity`, `porosity_value`, `adaptive_emissive_exposure_offset`, `aniso_lighting`, and `external_emittance`. They are parsed and dropped at the merge boundary with no comment marking the omission as deliberate — unlike `distance_field_alpha_texture` and the BGEM glass-overlay suite, whose gaps are at least annotated.
- **Impact**: None today. The wetness-control suite is the authored input the ROADMAP M61 wet-surface feature would need, so its silent absence is a scope trap rather than a rendering bug.
- **Related**: #2607, #2608, #2627, #2642, #2533.
- **Suggested Fix**: Add one grouped `// Deferred: no consumer` comment in `merge_external_material` naming these fields, so the next completeness sweep can tell "not yet wired" from "overlooked".

---

---
**Source**: `docs/audits/AUDIT_FO4_2026-08-12.md` (finding `FO4-D7-02`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs`, per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix

