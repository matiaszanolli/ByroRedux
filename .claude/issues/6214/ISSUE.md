# #6214: SKY-D2-01: EyeEnvmap reflection centres cross the NIFAL boundary in Gamebryo Z-up, and the shader places them with the Y-up instance matrix

**Labels**: high,bug,nifal,nif-parser,renderer,shaders,game:skyrim,game:fo4,legacy-compat

**Source**: docs/audits/AUDIT_SKYRIM_2026-10-21.md · **Severity**: HIGH · **Dimension**: 2 (Skyrim material slice) / NIFAL coordinate boundary · **Location**: `crates/nif/src/import/material/shader_data.rs:201-209`: `ShaderTypeData::EyeEnvmap` is copied verbatim into `ShaderTypeFields::eye_{left,right}_reflection_center`.

## Location
- `crates/nif/src/import/material/shader_data.rs:201-209`: `ShaderTypeData::EyeEnvmap` is copied verbatim into `ShaderTypeFields::eye_{left,right}_reflection_center`.
- `byroredux/src/material_translate.rs:744-747`: `shader_type_fields` is cloned, not converted.
- `byroredux/src/render/static_meshes.rs:300-313`: `pack_eye_fields` packs the values verbatim.
- `crates/renderer/shaders/triangle.frag`, the `if (isEyeEnvmap)` block: `leftWS = (instances[…].model * vec4(mat.eyeLeftCenterX, …, 1.0)).xyz`.

## Status at filing
NEW. Introduced with #5961 (`b4d073454`, closed). It is not REN-H-01: that finding is the missing `renderOrigin` term in the same two lines. This one is the axis convention of the inputs, and it survives REN-H-01's suggested fix. Searched open and closed issues for "eye centre", "EyeEnvmap", "reflection center": only #5961 and #430 (field plumbing) match.

## Description
every other positional quantity a NIF authors (vertices, node translations, bound centres) is converted once at import with `zup_to_yup_pos` (`(x, y, z) → (x, z, -y)`), so `GpuInstance.model` maps Y-up local space to world. The two eye reflection centres are the exception. They are authored in the eyeball shape's Z-up local space (nif.xml `BSLightingShaderProperty` Eye Envmap: Left/Right Eye Reflection Center), and no tier converts them, so the shader multiplies a Z-up point by a Y-up matrix. The centre's authored height becomes its forward depth, and its authored forward offset becomes height.

## Evidence
(real data, `meshes\actors\character\character assets\eyesmale.nif` from `Skyrim - Meshes0.bsa`, hand-decoded from `trace_block`) 
- Block 7 `BSLightingShaderProperty`, type `0x10`. Trailer: `eye_cubemap_scale` 1.3; left centre (−2.397, 7.572, 3.750); right centre (2.398, 7.572, 3.750).
- Block 3 `BSDynamicTriShape`, which has no shape transform: the first iris vertex is (−2.34, 8.54, 4.47) Z-up. The centre sits about 1 unit behind the iris surface, as expected.
- The correct Y-up centre is (−2.40, 3.75, −7.57). The shader uses (−2.40, 7.57, 3.75), about 11.9 units away, on the far side of the head along the facing axis.
- Once REN-H-01 is fixed, `normalize(fragWorldPos - centerWS)` is ≈ (0, −0.24, −0.97) for every iris fragment. That is a near-constant direction, which gives the flat reflection the branch exists to avoid.
- `material_dump` confirms the import (`kind 16`). `grep zup_to_yup crates/nif/src/import/material/` returns nothing.

## Impact
the "iris-centred" reflection degenerates to an almost fixed view direction per eye on every Skyrim (and FO4) eye with an authored cube. It is masked today by REN-H-01's larger origin error, so fixing REN-H-01 alone will look like the eye still does not work.

## Related
REN-H-01 (same block, different defect), SKY-D2-02 (same block), #5961, #430.

## Suggested Fix
convert both centres with `zup_to_yup_pos` where `ShaderTypeData::EyeEnvmap` is unpacked in `shader_data.rs`. That is the single import boundary, so FO4 / FO76 get the same fix. Add a test that feeds a known Z-up centre and asserts the Y-up value reaches `ShaderTypeFields`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/emitter.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
