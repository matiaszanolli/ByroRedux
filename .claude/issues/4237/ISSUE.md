# FO3-D1-2026-09-11-02: Window_Environment_Mapping/Eye_Environment_Mapping decoded but never reach the glass classifier

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/4237
**Labels**: bug, low, legacy-compat, game:fo3, nifal
**Source**: `/audit-fo3` — `docs/audits/AUDIT_FO3_2026-09-11.md`, finding FO3-D1-2026-09-11-02

**Severity**: LOW
**Dimension**: FO3 Rendering Path (Inline Shaders) — `/audit-fo3` Dimension 1
**Location**: `crates/nif/src/import/material/legacy_properties.rs:19-30` (`legacy_env_map_scale`), `crates/nif/src/shader_flags.rs:36-40` (`WINDOW_ENVIRONMENT_MAPPING`/`EYE_ENVIRONMENT_MAPPING`), `byroredux/src/material_translate.rs:649-666` (`classify_glass_into_material` call)

**Description**: `Window_Environment_Mapping`/`Eye_Environment_Mapping` are parsed and only feed `legacy_env_map_scale`'s on/off decision for `env_map_scale`; `classify_glass_into_material` never receives them, so window glass with a non-keyword-matching filename is misclassified as opaque dielectric instead of glass.

**Evidence**: `legacy_env_map_scale` (`legacy_properties.rs:19-30`) reads `WINDOW_ENVIRONMENT_MAPPING`/`EYE_ENVIRONMENT_MAPPING` (`shader_flags.rs:36-40`) only to gate `env_map_scale`. The `classify_glass_into_material` call site (`material_translate.rs:649-666`) has no parameter carrying either flag.

**Impact**: Visual only, partially masked by filename-keyword classification (most vanilla window meshes happen to match a glass keyword).

**Suggested Fix**: Carry `window_env_mapping: bool` through `MaterialInfo`/`ImportedMaterial` as an additional positive signal into `classify_glass_into_material`.

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: Signal must land at the NIF import → `Material` boundary, not a render-time keyword rescan. See `/audit-nifal`.
- [ ] **TESTS**: A regression test on a synthetic non-keyword-named window mesh with `Window_Environment_Mapping` set pins correct glass classification.
