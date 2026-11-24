# #7408 SK-D2-01: `apply_shader_type_data` doc still says EyeEnvmap and MLP `refraction_scale` are "packed into the GPU material but not yet shaded (#5961)"

**Labels**: low,nifal,documentation,doc-rot,game:skyrim,legacy-compat

**Source**: `docs/audits/AUDIT_SKYRIM_2026-11-23.md` — finding `SK-D2-01` (HEAD `e01ca71f0`, re-validated against `origin/main` `4e68ab7b7` at publish).

- **Severity**: LOW
- **Dimension**: Shader-type dispatch / Skyrim material slice (doc-rot)
- **Location**: `crates/nif/src/import/material/shader_data.rs:99-101` (rustdoc of `apply_shader_type_data`)
- **Status**: NEW (closed #3908 is the older sweep, predating #5961)
- **Description**: b4d073454 (#5961) shades both; #6213 / #6261 refined the eye path. The doc still says neither is shaded.
- **Evidence**: doc text "EyeEnvmap (16) and MultiLayerParallax's `refraction_scale` are packed into the GPU material but not yet shaded (#5961)"; live consumers `triangle.frag` `isEyeEnvmap` (~3147, branch ~3361-3393) and the `MLP_REFRACTION_UV_PER_UNIT` refract offset (~1575-1584).
- **Impact**: none at runtime; misleads a reader of the import boundary into thinking two Skyrim shader types are render-incomplete.
- **Related**: #5961, #6213, #6261, #3908
- **Suggested Fix**: name the live consumers (the EyeEnvmap branch gated by `pack_eye_fields`; the MLP refraction UV offset).

## Completeness Checks
- [ ] **SIBLING**: other "not yet shaded (#5961)" wording grepped and corrected
