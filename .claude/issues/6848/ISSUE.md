# FO4-2026-11-10-D2-04: the #2704 / #4667 "parsed, no sink" ledger is incomplete

Labels: low,documentation,doc-rot,import-pipeline,game:fo4,legacy-compat
Source: docs/audits/AUDIT_FO4_2026-11-10.md

- **Severity**: LOW
- **Dimension**: 2 (parsed-then-dropped ledger; no visual claim)
- **Location**: ledger comment at `byroredux/src/asset_provider/material/merge.rs:1473-1510`; fields in `crates/bgsm/src/base.rs`, `bgsm.rs`, `bgem.rs`
- **Status**: NEW (no matching open or closed issue; #4667 is the ledger's origin)
- **Description**: the ledger exists so "the next completeness sweep can tell not-yet-wired from overlooked". Fields that are parsed, have **no** `ImportedMaterial` hop, and are absent from the ledger (probe 2, vanilla `Fallout4 - Materials.ba2`, authored non-default count in parentheses):
  - BGSM: `base.z_buffer_write = false` (17), `base.z_buffer_test = false` (1), `base.screen_space_reflections` (17), `wetness_control_ssr` (2), `base.non_occluder` (138), `hair` (18) + `hair_tint_color`, `skin_tint` (20), `facegen` (12), `tree` (60), `tessellate` (70), `skew_specular_alpha` (1), `assume_shadowmask` (12), `enable_editor_alpha_ref` (258), `refraction_falloff` (7).
  - BGEM: `z_buffer_write = true` (40 of 283), `z_buffer_test = false` (1), `emittance_color`, `blood_enabled` / `falloff_enabled` (read only by the glass predicate), `adaptive_emissive_*`.
  - `distance_field_alpha_texture` (#2642) is documented elsewhere and unchanged.
- **Evidence**: the depth pair is the only group with a canonical sink on the other side: `Material.z_write` / `z_test` are copied by `material_translate.rs:744` and drive `cmd_set_depth_write_enable` per batch (`geometry_pass.rs:473,942`), but FO4 `BSTriShape` NIFs carry no `NiZBufferProperty`, so for FO4 the BGSM/BGEM pair is the only authored source and neither arm writes `material.z_write`. Of the 17 BGSM `z_write = false` files, 8 are blended decals that the engine already forces to `z_write = false` (`render/static_meshes.rs:933`); the other 9 are alpha-tested or non-blended (`hairshort_lgrad_1bit`, `malehairmedium`, `wig_lgrad_1bit`, `femalehairlong`, `hairlong_lgrad_1bit`, `humancommon/hairshort_lgrad_1bit`, `signstheater01`, `minigunammobelt`, `baseballbatbloodloadscreen`). BGEM goes through `BSEffectShaderProperty`, which `dedicated_shader.rs:732` already sets to `z_write = false` by convention.
- **Impact**: none demonstrated. This is a ledger/documentation gap, not a rendering claim (no capture was made; per the no-guessing policy the hair/sign draws are the first thing to look at if a depth artefact is ever seen on them). Severity LOW.
- **Suggested Fix**: extend the ledger comment (or record the groups in `docs/engine/nifal.md`) with the list and counts above; wire `z_buffer_write/test` only after a capture shows a difference.
