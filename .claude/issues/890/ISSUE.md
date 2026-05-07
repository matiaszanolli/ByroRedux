# #890 — SK-D4-NEW-04: BSEffect SOFT_EFFECT / GREYSCALE_TO_PALETTE_* / EFFECT_LIGHTING bits captured but not consumed

**Audit**: `docs/audits/AUDIT_SKYRIM_2026-05-06_DIM1_4.md` (Dim 4)
**Severity**: LOW
**Labels**: low, nif-parser, import-pipeline, renderer, bug
**Created**: 2026-05-07

## Sites
- `crates/nif/src/import/material/mod.rs:549-583` (`BsEffectShaderData` struct)
- `crates/nif/src/import/material/shader_data.rs:11+` (`capture_effect_shader_data`)
- `crates/nif/src/import/material/walker.rs:340-380` (call site)
- `crates/nif/src/shader_flags.rs:201+` (helper pattern source — `is_two_sided_from_modern_shader_flags`)

## Summary
`BSEffectShaderProperty` parsing captures `shader_flags_1/2 + sf1_crcs/sf2_crcs` correctly. The Skyrim/FO4 two-sided + decal helpers run on the import side. But there is no consumer for SLSF1 bits 0x40 (SOFT_EFFECT), 0x80 (GREYSCALE_TO_PALETTE_COLOR), 0x100 (GREYSCALE_TO_PALETTE_ALPHA), or SLSF2 bit 0x100 (EFFECT_LIGHTING). `BsEffectShaderData` has typed fields for `soft_falloff_depth`/`greyscale_texture`/`lighting_influence` but no boolean for the flag bits. Explains a known visual gap on Skyrim spell FX (greyscale renders as raw luminance, soft particles hard-cut).

## Fix (Stage 1 — this issue, import-side capture only)
1. Add booleans to `BsEffectShaderData`: `effect_soft`, `effect_palette_color`, `effect_palette_alpha`, `effect_lit`.
2. Populate via a typed-flag-or-CRC helper modeled on `is_two_sided_from_modern_shader_flags`.
3. Optionally mirror to `MaterialInfo` if downstream needs cheap top-level access (parallels `texture_clamp_mode` mirror at `walker.rs:379`).

Stage 2 (separate issue): `triangle.frag` `MATERIAL_KIND_EFFECT_SHADER` consumers — soft-particle depth read, palette LUT sampler binding, simple lit-effect path. Non-trivial; defer.

## Completeness
- SIBLING: verify new BSEffect helpers don't re-flag bits with incompatible meaning on the BLSP side.
- TESTS: extend `effect_shader_capture_tests.rs` (already exists per `material/mod.rs:733`) — typed-flag path + CRC-fallback path for each of the 4 bits, plus negative test.
