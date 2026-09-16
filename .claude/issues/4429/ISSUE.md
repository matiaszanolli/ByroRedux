# #4429: SF-2026-09-16-D3-01: The canonical texture-role vocabulary has no role for Starfield's roughness / metalness / AO / opacity / transmissive maps — CDB Phase 2 has nowhere correct to put ~39% of Starfield's textures

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4429
- **Labels**: medium,nifal,renderer,game:starfield,legacy-compat,bug
- **Filed**: 2026-09-16 via /audit-publish

**Source**: `docs/audits/AUDIT_STARFIELD_2026-09-16.md` (texture-roles-deep audit suite, HEAD `7996edf61`)

- **Severity**: MEDIUM
- **Dimension**: 3 (CDB material correctness) / 9 (`.mat` → `MaterialTextureSet` roles)
- **Location**:
  - `crates/nif/src/import/types.rs:335-369` (`MaterialTextureSet`, 22 roles + 4 decals)
  - `crates/renderer/src/vulkan/material.rs:569-585` (`supplemental_texture_slot`, 16 lanes)
  - `crates/renderer/shaders/triangle.frag:1437-1452` (gloss-map sampling)
  - `docs/audits/SF_CDB_PHASE2_SPIKE_2026-08-29.md:160-163, 226-229`
- **Status**: NEW. #3398 item 2 asks for "a field-name → `MaterialTextureSet`
  role … mapping" and the spike tabulates `MRTextureFile`/`TextureFile` →
  "texture roles". Neither records that the role vocabulary itself is missing
  the destinations. The `nifal.md` parked-roles table does not list them either.
- **Description**: Starfield authors separate single-channel PBR maps. Its
  texture archives and its own TXST records show `_rough`, `_metal`, `_ao`,
  `_opacity` and `_transmissive` as first-class kinds, each in its own TXST
  slot (TX09 / TX08 / TX17 / TX19). `MaterialTextureSet<T>` has 22 roles and
  none of them is roughness, metalness, ambient occlusion, opacity or
  transmission. `GpuMaterial` has no lane for them, and `triangle.frag`
  samples none of them.

  The only nearby role is `smooth_spec` (legacy gloss / BGSM smooth-spec). The
  shader consumes it as **gloss**: `roughness = mix(1.0, roughness,
  glossTexel.r)`. That is the inverse of a roughness map, so routing `_rough`
  there is a silent sign flip. `specular` is a colour map, not metalness.

  So when Phase 2 lands per-texture extraction, each non-colour/normal texture
  has three options, and all three break a project rule:
  1. drop it silently (NIFAL "no silent drop");
  2. misroute it into a role with different semantics ("no fabrication", and
     the inverted gloss);
  3. store it under a CDB-specific index, which the checklist forbids ("never
     a CDB slot index").

  The spike's §4 step 5 ("The merge arm … mirrors the BGSM arm directly; ~80
  lines") therefore understates the work. The canonical roles, the
  `GpuMaterial` lanes (a lockstep `bindings.glsl` change against the 428 B
  pin) and the shader consumers are a prerequisite, not a follow-up.
- **Evidence**:
  - **Texture-archive suffix census** (45,756 files): `_rough` 7,548, `_ao`
    4,990, `_opacity` 2,790, `_metal` 2,357, `_transmissive` 313. That is
    17,998 files (39%) with no canonical role.
  - **`Starfield.esm` TXST census** (23 records): TX08 → `_metal` (11), TX09 →
    `_rough` (11), TX17 → `_ao` (1), TX19 → `_opacity` (16).
  - **`roles()`** (`types.rs:384-413`) enumerates every role. None of these
    kinds appears.
  - **Merge-arm comment**: `merge.rs:544-545` already records the metalness
    half for FO4 ("Per-texel metalness from the spec map … deferred — needs a
    metalness-map shader binding"). It was scoped as an FO4 refinement, not as
    a Starfield Phase-2 blocker.
- **Impact**:
  - **Today**: none. Zero Starfield texture roles are produced (see the
    Executive Summary).
  - **At Phase 2**: the fidelity step #3398 exists to deliver would land at
    most `_color` / `_normal` / `_emissive` / `_height`. Every Starfield
    surface would keep scalar-only roughness and metalness and would have no
    AO and no opacity mask. Opacity-driven decals and cutouts, such as the 16
    `_opacity` TXST decals (`decalpuddlemd01_opacity.dds`), have no coverage
    source.
- **Related**:
  - #3398 (Phase 2 tracker).
  - #4277 (loose `.mat` JSON resolver, which will need the same roles).
  - SF-2026-09-16-D5-01 (the TXST decode drops the same four slots).
  - `mat_path_forwards_no_texture_roles_until_cdb_phase_2_lands` pins
    zero-forwarding only. Phase 2 must rewrite it, so the "never a CDB slot
    index" half of the invariant has no guard that survives Phase 2.
- **Suggested Fix**: Before the Phase-2 merge arm, add canonical roles
  (roughness, metalness, ambient_occlusion, opacity, transmissive) with
  documented channel semantics, their `GpuMaterial` lanes and shader
  consumers. Or record them explicitly as parked in `docs/engine/nifal.md`
  with #3398 as the unblocking consumer. In the same change, add a guard that
  a CDB-sourced role lands by name.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other games' arms)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs`, per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **SHADER-SYNC**: `triangle.frag` and `include/ray_hit.glsl` (`rayHitAlbedo`) change in lockstep; `.spv` recompiled with plain `-V`
- [ ] **TESTS**: A regression test pins this specific fix
