# FO4-D6-C: crates/bgsm/ parser missing — external material files unreachable

**Issue**: #411 — https://github.com/matiaszanolli/ByroRedux/issues/411
**Labels**: bug, renderer, high, legacy-compat

---

## Finding

Fallout 4 stores PBR-ish material parameters in external `.bgsm` (lit) and `.bgem` (effect) files. `BSLightingShaderProperty.net.name` holds the path. The NIF importer stores `material_path` for diagnostics (and the `mesh.info` debug CLI surfaces it) but **no BGSM parser exists**.

`Fallout4 - Materials.ba2` (BA2 v8) contains 6,616 BGSM + 283 BGEM files. DLCs add more. Mods ship their own.

## BGSM v2 format (verified from live samples)

All binary, no JSON in vanilla. Magic + u32 version (FO4=2, FO76=20, Starfield=21/22).

**Common prefix (BGSM v2 @ offset 0x00-0x3E)**:
- `tile_flags` u32 (TileU/TileV/IsTile bits)
- `uv_offset` [f32;2]
- `uv_scale` [f32;2]
- `alpha` f32
- `alpha_blend` u8 + `src_blend` u32 + `dst_blend` u32 + `alpha_threshold` u8 + `alpha_test` u8
- `z_write` u8 + `z_test` u8 + `SSR`/`wetness_ssr` u8 + 7× u8 flags (decal/two_sided/non_occluder/refraction/...)
- `refraction_power` f32 + `env_mapping` u8 + `env_map_scale` f32

**BGSM lit texture slots (9)**: diffuse, normal, `_s.dds` specular/smooth, greyscale, envmap, glowmap, env_mask, unused×2.

**BGSM lit trailer**:
- `emittance_*`, 11× u8 material flags (modelspace_normals, external_emittance, back_lighting, receive_shadows, cast_shadows, dissolve_fade, glowmap_enabled, env_window, env_eye, etc.)
- `hair_tint_color` / `grayscale_tint_color` / `specular_color` [f32;3] × 3
- `specular_mult` / `smoothness` / `fresnel_power` f32 × 3
- `wetness_control` [f32;4]
- **`rootmaterial_path`** length-prefixed string — **template inheritance**
- `anisolighting` / `emit_enabled` / `subsurface_lighting` u8 + `emit_color` [f32;4] + `emit_mult` f32 + `subsurface_rolloff` / `specular_lighting_power` / `grayscale_to_palette_scale` f32 × 3

**BGEM v2**: shares the common 0x00-0x3E prefix and texture slots, then diverges into `base_color[3]`, `base_color_scale`, `falloff_enabled` + 4 floats, `soft` + depth. Matches the existing `BSEffectShaderProperty` trailing fields.

Authoritative reference: niftools `nifly/BGSM.h`. Samples verified at `/tmp/audit/fo4/sample_{0,1,2}.bgsm` + `sample_{0,1}.bgem`.

## Template inheritance is mandatory

Every creature BGSM points at `template/CreatureTemplate_Wet.bgsm` via `rootmaterial_path`. A BGSM parser must **recurse through this chain and merge parent fields bottom-up** (child overrides parent), or parsed values are wrong for nearly all actor materials. An LRU cache on the parsed template graph is mandatory — otherwise inheritance dominates load time.

## Proposed crate placement

**New `crates/bgsm/` crate.** Rationale:
- Standalone binary format, independent of both NIF and plugin.
- Has its own version matrix (FO4 v2, FO76 v20, Starfield v21/22).
- Only two downstream consumers: `crates/nif/src/import/material.rs` (optional dep, fills `MaterialInfo` when `material_path` is present) and `byroredux/src/asset_provider.rs` (the file read).
- Mirrors `crates/bsa` factoring.

A slim `trait MaterialSource` in `core` lets NIF Phong props and external BGSM funnel into the same `Material` component without either layer knowing about the other.

## Companion work: GpuInstance + shader expansion

**Parsing BGSM alone lands ~30% of fields on the GPU.** The following BGSM fields have no `GpuInstance` slot today and need parallel plumbing:
- `uv_offset[2]` / `uv_scale[2]` — every BGSM-textured mesh authors UV transforms
- `material_alpha` — discard-driven transparency
- Specular (`_s.dds`), glowmap, greyscale, envmap, env_mask texture slots
- Subsurface / rimlight / backlight / fresnel / wetness / tint family
- BGEM falloff + soft-depth

Without a parallel `GpuInstance` expansion + fragment-shader lockstep (respecting [feedback_shader_struct_sync.md] — all 3 shaders updated together), BGSM parsing will not visibly improve FO4 renders.

## Fix — staged

1. **Parser** (1-2 weeks): new `crates/bgsm/` with BGSM v2 + BGEM v2 reader. Include `rootmaterial_path` template-inheritance resolver with LRU cache. Defer FO76 v20 / Starfield v21/22 to a follow-up.
2. **GpuInstance extension**: add `uv_offset_u/v`, `uv_scale_u/v`, `material_alpha` (4 new f32; 160 B → 176 B, pad to 192 B = 12×16). Update `triangle.vert`, `triangle.frag`, `ui.vert` in lockstep. Keep the regression test at `scene_buffer.rs:820-857` green.
3. **asset_provider BGSM resolver**: when `material_path` ends in `.bgsm` / `.bgem` and `texture_path` is empty, open Materials BA2 → parse with template resolution → populate `MaterialInfo`. Fall back to NIF defaults on parse failure.
4. **Fragment shader**: apply `uv_offset` + `uv_scale` to every sample, multiply `material_alpha` into pre-discard alpha.
5. **Corpus test**: `crates/bgsm/tests/parse_all.rs` with 95% threshold on all 6,899 vanilla BGSM/BGEM (realistic: 99%+).

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: TXST MNAM path handling (FO4-D4-C3) — the BGSM resolver must also be reachable from the TXST lookup table.
- [ ] **DROP**: If GpuInstance grows, regression test at `scene_buffer.rs:820-857` must still pass.
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Per feedback_shader_struct_sync.md — all 3 shaders updated in lockstep; integration test for a BGSM-referenced mesh with non-identity uv_offset renders correctly.

## Source

Audit: `docs/audits/AUDIT_FO4_2026-04-17.md`, Dim 6 Section 1-3 + Stage C.
