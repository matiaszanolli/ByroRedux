# REN-D15-05: water.frag ampScale/freqScale sentinels duplicated with no lockstep guard, test is tautological

## Description
The #2240 normalisation sentinels (`0.05`, `0.6`) are hard-coded literals in the shader and duplicated in two Rust `Default` impls with no lockstep guard — though the `WATER_CALM…WATERFALL` enum values used by the *same shader* already go through `shader_constants_data.rs` + the #1780 include test. Worse, the test named for the contract (`default_wave_params_are_the_sentinel_the_shader_normalises_against`) passes the values in **explicitly** rather than reading `WaterMaterial::default()` — a pass-through tautology.

## Location
`crates/renderer/shaders/water.frag` (`ampScale` / `freqScale` divisors), `crates/core/src/ecs/components/water.rs`, `crates/plugin/src/esm/records/misc/water.rs`, `byroredux/src/render/water_wave_params_tests.rs`

## Severity / Domain / Type
low / renderer / bug

https://github.com/matiaszanolli/ByroRedux/issues/2787

Filed from docs/audits/AUDIT_RENDERER_2026-08-12b.md (finding REN-D15-05).
