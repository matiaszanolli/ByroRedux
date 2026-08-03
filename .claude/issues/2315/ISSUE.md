# FO3-D1-01: env_map_scale forwarded from FO3 inline shaders with no Environment_Mapping flag gate

Filed from: `docs/audits/AUDIT_FO3_2026-08-03.md`
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2315

**Severity**: HIGH
**Location**: `crates/nif/src/import/material/legacy_properties.rs:325` (PPLighting), `:386` (NoLighting), `:445`/`:452`/`:458`/`:469` (Tile/Sky/TallGrass/Water), `:490` (BaseOnly); consumed at `crates/core/src/ecs/components/material.rs:647-671`
**Status**: NEW (sibling of CLOSED #1873 "chrome flyer" — that fix added `specular_authored`; this is the un-gated *input* reaching the same arm)

### Description
`apply_pp_lighting_property` writes `info.env_map_scale = shader.shader.env_map_scale` unconditionally. Per nif.xml the field defaults to 1.0 and is only meaningful when F3SF1 bit 7 `Environment_Mapping` (or bit 21/17 window/eye variants) is set — none of which `fo3nv_f1` even declares. Essentially every FO3 `BSShaderPPLightingProperty` mesh exits import with `env_map_scale ≈ 1.0` regardless of authored intent.

Confirmed against current code: all seven legacy shader branches (`legacy_properties.rs:325,386,445,452,458,469,490`) write `info.env_map_scale = shader.shader.env_map_scale` with no flag gate. `crates/core/src/ecs/components/material.rs:647` (`if inputs.env_map_scale > 0.3`) is the metalness/roughness classifier this feeds. `ENVIRONMENT_MAPPING`/`EYE_ENVIRONMENT_MAPPING` constants already exist in `crates/nif/src/shader_flags.rs:134,147` but are not referenced from `legacy_properties.rs`.

### Evidence
Real FO3 data: FO3 statics carrying both `NiMaterialProperty` + `BSShaderPPLightingProperty` (the common vanilla shape) classify as 0.4-metalness/0.55-roughness semi-conductors — under the RT reflection gate they pick up environment response on plaster, concrete, painted metal, wood. Same visual class as closed #1873, reached via the un-gated input.

### Impact
FO3 statics with the common `NiMaterialProperty`+PPLighting pairing misclassify as semi-metallic, picking up unauthored environment reflections. Also permanently disables the Gamebryo normal-alpha-as-spec path (`normal_alpha_spec_applies` requires `env_map_scale <= 0.3`). Shared with FNV; FO3's heavier `NiMaterialProperty`+PPLighting pairing makes it the dominant population.

### Suggested Fix
Add `ENVIRONMENT_MAPPING`/`WINDOW_ENVIRONMENT_MAPPING`/`EYE_ENVIRONMENT_MAPPING` (bits 7/21/17) to `shader_flags::fo3nv_f1` and gate the `env_map_scale` write on them; leave at 0.0 otherwise.

### Related
#1873 (CLOSED, `634873db`), `feedback_chrome_means_missing_textures.md`, FO3-D1-02

## Completeness Checks
- [ ] **SIBLING**: Shared code path with FNV/Oblivion/Skyrim — same fix applies there too (verify no per-game divergence was introduced)
- [ ] **CANONICAL-BOUNDARY**: Fix touches the NIFAL parser→`Material` boundary (`legacy_properties.rs` → `MaterialInfo`) — gate stays at import time, not pushed into the shader. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins "FO3 PPLighting mesh without Environment_Mapping flag ⇒ env_map_scale == 0.0"
