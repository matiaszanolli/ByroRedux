# SKY-2026-09-11-D2-02: env_map_scale_consumed latch not set by the two Skyrim+ dedicated shader writers

Issue: https://github.com/matiaszanolli/ByroRedux/issues/4251

**Severity**: LOW
**Dimension**: 2 — BSLightingShaderProperty / BSEffectShaderProperty Shader-Type Dispatch
**Location**: `crates/nif/src/import/material/shader_data.rs` (EnvironmentMap arm), `crates/nif/src/import/material/dedicated_shader.rs` (effect-shader path)
**Status**: NEW

**Description**: The #2328 `env_map_scale_consumed` latch protecting a dedicated shader's value from being clobbered by a later legacy property is only honoured by the legacy writers; the two Skyrim+ dedicated writers (`shader_data.rs` EnvironmentMap arm, `dedicated_shader.rs` effect-shader path) never set the latch. Same residual shape as the already-closed #3514/#3517 for `refraction_strength`/`texture_clamp_mode`.

**Evidence**: Confirmed by inspection — the `env_map_scale_consumed` latch is set by legacy-property writers but neither the Skyrim+ `EnvironmentMap` dedicated-shader arm in `shader_data.rs` nor the effect-shader path in `dedicated_shader.rs` sets it, mirroring the pattern #3514/#3517 fixed for two sibling fields.

**Impact**: Currently unreachable on vanilla Skyrim content — same latent shape as the two closed sibling bugs before their fix. A NIF whose blocks are ordered with the legacy property after the dedicated shader would clobber the dedicated value.

**Related**: #2328 (introduced the latch), #3514, #3517 (fixed the same gap for `refraction_strength`/`texture_clamp_mode`).

**Suggested Fix**: Set `env_map_scale_consumed` in both dedicated-shader writers, mirroring the #3514/#3517 fix shape for the sibling fields.

## Completeness Checks
- [ ] **SIBLING**: Confirm no other dedicated-shader writer omits setting a "consumed" latch its legacy-writer sibling honours
- [ ] **TESTS**: A regression test with block order [dedicated shader, then legacy property] pins that the dedicated `env_map_scale` value survives
