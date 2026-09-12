# Batch: #4249, #4250, #4251, #4252

## #4249 — SKY-D1-2026-09-11-01: BSTriShape particle-data trailing read gated on bsver < FALLOUT4 where nif.xml gates it on exact BSVER 100
**Severity**: LOW · **Location**: `crates/nif/src/blocks/tri_shape/bs_tri_shape.rs:659-668`

Gate is `bsver < FALLOUT4` (broad) where nif.xml's `#BS_SSE#` is exactly BSVER 100. No vanilla
content reaches the gap. Fix: narrow to exact BSVER == 100.

## #4250 — SKY-2026-09-11-D2-01: Skyrim BSEffectShaderProperty import writes env_map_scale = 0.0 where MaterialInfo's own default is 1.0
**Severity**: LOW · **Location**: `crates/nif/src/import/material/dedicated_shader.rs:578`

Skyrim's absent-env_map_scale path hardcodes 0.0 instead of MaterialInfo's own declared 1.0
default. Currently latent (material_kind=101 short-circuits shading first).

## #4251 — SKY-2026-09-11-D2-02: env_map_scale_consumed latch not set by the two Skyrim+ dedicated shader writers
**Severity**: LOW · **Location**: `crates/nif/src/import/material/shader_data.rs`, `dedicated_shader.rs`

Same shape as already-closed #3514/#3517 for refraction_strength/texture_clamp_mode — the latch
protecting a dedicated shader value from being clobbered by a later legacy property isn't set by
the two Skyrim+ dedicated writers.

## #4252 — SKY-2026-09-11-D2-03: only shader_type=0 of thirteen no-trailing-data Skyrim shader types has a wire-level byte-position test
**Severity**: LOW · **Location**: `crates/nif/src/blocks/shader.rs`; `crates/nif/src/import/tests/`

Pure test-gap. Add wire-level byte-position assertions for the remaining twelve None-mapped
shader-type values (2,3,4,8,9,10,12,13,15,17,18,19,20).
