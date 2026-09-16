# Renderer Audit — 2026-09-16 (NARROWED: Dimensions 6 + 7)

`/audit-renderer --focus 6,7`, run as part of the `texture-roles-deep` audit-suite
preset. Scope:
- **Dimension 6**: the NIFAL material canonical translation.
- **Dimension 7**: the R1 material table.

The preset asks how the renderer *consumes* texture roles and material data after
the 2026-07-27 cross-game texture-role unification (`1d94eb24`, `05d68926`,
`c8c8a834`). That covers the `GpuMaterial` layout, dedup, the SSBO upload, colour
space per role, and sampler addressing per role. Live tree at HEAD `7996edf61`.

Baselines:
- `docs/audits/AUDIT_RENDERER_2026-09-14.md` (full sweep; its D6 findings are all
  closed except #4304).
- `docs/audits/AUDIT_RENDERER_2026-09-05_DIM6_DIM7.md` (its six findings are all
  closed: #3902, #3908–#3912).

Sibling reports from today that this report does **not** repeat:
- `docs/audits/AUDIT_NIFAL_2026-09-16.md`:
  - D8-01: flipbook frames bypass the per-role resolver.
  - D8-02: flipbook frames are never released.
  - D8-03: FO4 slot-2 `Glow_Map` gate.
  - D8-04: Starfield colocated-slot vocabulary.
  - D8-05: gap in the colour-space test.
  - D8-06: stale `specular` comment.
- `docs/audits/AUDIT_FO4_2026-09-16.md`:
  - FO4-D5-01: `_s.dds` bound as both gloss and specular colour.
  - FO4-D2-01, D2-02, D2-03: BGEM palette bits, `environment_mapping` gate,
    and REFR slot routing.

All work was done in this session, with no sub-agents. No source was modified, and
the engine binary was not launched.

Every finding was re-checked against the code at HEAD. Where a claim depends on
what the content contains, it was measured against the vanilla archives. The
measurement used a throwaway census program, built in the session scratchpad
(outside the repo) against `byroredux-nif`, `byroredux-bsa` and `byroredux-core`.
It works in two steps:
1. Import every mesh through `import_nif`.
2. Extract the role textures and fully decode their BC1 blocks.

## Executive Summary

**7 new findings: 0 CRITICAL, 3 HIGH, 0 MEDIUM, 4 LOW.**

The material table itself is healthy:
- **Dedup identity is now a byte-hash.** `hash_gpu_material_fields` hashes the
  struct's full byte view (`479ce5266`, #4201, landed today). A new test pins
  coverage of every byte.
- **Layout still matches the shader mirror.** `GpuMaterial` is 428 B and its
  field-offset pins pass.
- **Upload is correctly bounded.** Over-cap interns return id 0, and the upload
  is capped at the unique-material count.

The problems are in **how three Skyrim actor texture roles are consumed**. All
three were measured on vanilla content, and all three hit the same surface:
Skyrim NPC skin.

1. **REN-2026-09-16-D7-01 (HIGH): the `detail` role darkens every Skyrim NPC face
   to about 11%.** The shader applies the Gamebryo ×2 detail modulate after an
   sRGB decode. Vanilla Skyrim's own `blankdetailmap.dds` is a uniform 65/255, so
   the "no detail" map multiplies albedo by 0.106. The role is live on all 3,149
   vanilla FaceGeom head shapes, and on 0 shapes in Oblivion, FO3 and FNV.
2. **REN-2026-09-16-D7-02 (HIGH): the `tint` role multiplies Skyrim skin albedo
   by the `_sk.dds` mask.** That mask is DXT1, so its alpha is 1 everywhere and
   the full multiply always applies. Vanilla `malehead_sk.dds` decodes to a
   multiplier of (0.086, 0.021, 0.016). It is live on 4,100 vanilla shapes:
   heads, bodies and hands.

   Together with D7-01, a male NPC face reaches the lighting stage at roughly
   (0.009, 0.002, 0.002) × its diffuse. Both multiplies are repeated on
   secondary rays (`rayHitAlbedo`).
3. **REN-2026-09-16-D6-01 (HIGH, regression of #2095): the per-NPC FaceGen tint
   DDS is applied to the wrong submeshes.** `select_facegen_diffuse` gates the
   override on `material_kind == 5` (SkinTint). All 3,149 vanilla FaceGeom *head*
   shapes are kind 4 (FaceTint). The kind-5 shapes inside FaceGeom NIFs are
   scars, Argonian hair and Orc tusks. The per-NPC face atlas therefore replaces
   those textures, which is the "mush" the gate was written to prevent, while
   the head keeps the generic race diffuse.

The four LOW findings are hygiene:
- stale dedup prose and a double struct build left behind by today's #4201;
- the `GpuMaterial` offset comments, still off by 4 bytes since #3909;
- parallax and IOR default literals that bypass the named canonical constants;
- three separate `unsafe` byte views of `GpuMaterial`, where one `NoUninit`-bound
  helper already exists.

None of the three HIGH findings has been seen in a running engine. They follow
from the measured texel data and the shader arithmetic. See **Needs visual
confirmation** for the one-flag A/B that checks D7-01.

## RT Pipeline Assessment (scope-limited)

- **Primary and secondary shading agree.** `rayHitAlbedo`
  (`crates/renderer/shaders/include/ray_hit.glsl`) applies the same
  decal → diffuse → tint → inner layer → dark → detail chain as `crates/renderer/shaders/triangle.frag`
  (#3902 holds). As a result, D7-01 and D7-02 also darken what RT reflections and
  GI see of Skyrim actors.
  - One divergence: the secondary path ignores `DBG_BYPASS_DETAIL`. That matters
    only for the A/B in the Needs visual confirmation section.
- **Role indices are safe on the GPU.** Every sampled role is gated on
  `!= 0u` before `textures[nonuniformEXT(...)]` is indexed. Handle 0 is the
  registry's checker fallback, and `resolve_material_texture_handles_with_clamp`
  maps an authored-but-missing secondary role to 0, so a missing file cannot
  reach a sampler. The two packed high bits are masked before indexing:
  `PARALLAX_ALPHA_HEIGHT_BIT` and `NORMAL_ALPHA_SPEC_BIT`.
- **Cubemaps use their own array.** `env_map_index` indexes the separate
  `cubemaps[]` array. It comes from `resolve_environment_texture_with_clamp`, and
  its cache key carries `|cube`.

## GPU-Struct & Memory Assessment (scope-limited)

- **`GpuMaterial` layout: PASS.** Size is 428 B. `gpu_material_size_is_428_bytes`
  and `gpu_material_field_offsets_match_shader_contract` are green. The GLSL
  mirror in `crates/renderer/shaders/include/bindings.glsl` matches field by
  field: 107 scalar fields, none of them `vec3`.
- **Dedup key: PASS (changed today).** Since #4201, `hash_gpu_material_fields` is
  `FxHasher::write(mat.as_bytes())`. `DrawCommand::material_hash` is now defined
  as that hash of `to_gpu_material()`, so the two can no longer disagree.
  Coverage is pinned by `hash_gpu_material_fields_covers_every_gpu_material_field`,
  which XORs each of the 428 bytes. It also keeps a source pin requiring
  `h.write(mat.as_bytes());`. `material_hash_matches_gpu_material_field_hash` is
  kept as a guard against a future second walk.
- **Upload: PASS.** `upload_materials` hard-asserts `len <= MAX_MATERIALS` and
  writes `materials[..min(len, MAX_MATERIALS)]` (#4045 holds). The copy is skipped
  through the `hash_material_slice` dirty-gate.
- **Telemetry: PASS.** `unique_user_count`, `interned_count` and `overflow_count`
  are published every frame in `byroredux/src/app_frame.rs`, and the overflow
  count is debug-asserted to be zero.
- **Colour space per role.** The table in `map_secondary_texture_handles`
  (`byroredux/src/asset_provider/texture.rs`) is internally consistent: numeric
  maps are Linear and colour maps are sRGB. The registry keeps Linear and sRGB
  views of one path apart through the `|linear` suffix in
  `texture_keyed_path_with_color_space`.

  What this sweep adds is the *consumer* side. For two multiplicative roles
  (`detail`, `tint`), the shader arithmetic assumes a neutral texel value that
  the sRGB-decoded vanilla textures do not have (D7-01, D7-02). The NIFAL test
  gap (D8-05) is not repeated here.
- **Sampler addressing per role.** Every role shares the material's single
  `texture_clamp_mode`, and the registry caches one entry per
  `(path, clamp, view, colour space)`. The flipbook exception is NIFAL D8-01 and
  is not repeated here.

## Findings

### HIGH

#### REN-2026-09-16-D7-01: The detail role multiplies albedo by 2 × an sRGB-decoded sample, so vanilla Skyrim's own "blank" detail map darkens every NPC face to ≈11%
- **Severity**: HIGH. The surface colour is wrong on every Skyrim NPC head, the
  secondary-ray albedo is wrong with it, and nothing in the draw path compensates.
- **Dimension**: Material Table (texture-role consumption)
- **Game Affected**: Skyrim (live on 3,149 of 3,149 vanilla FaceGeom head shapes).
  Oblivion, FO3 and FNV have no live occurrence: the census below finds 0 `detail`
  roles in their vanilla mesh archives.
- **Location**:
  - `crates/renderer/shaders/triangle.frag`: the `mat.detailMapIndex` block,
    `albedo *= detailSample * 2.0`.
  - `crates/renderer/shaders/include/ray_hit.glsl`: `rayHitAlbedo`,
    `rgb *= detailSample * 2.0`.
  - The colour-space choice is in `map_secondary_texture_handles`
    (`byroredux/src/asset_provider/texture.rs`): `detail: slot(&textures.detail, srgb)`.
  - The producer is the `(TextureSlotLayout::Skyrim, 3)` arm of `slot_to_role`
    (`crates/nif/src/import/material/slot_role.rs`): FaceTint slot 3 →
    `TextureRole::Detail` (#2694).
- **Status**: NEW. The ×2 dates from #399 (`c2cfcaa35`, 2026-04-19) and was
  written for Oblivion's `NiTexturingProperty` slot 2. The first live producer is
  #2694's FaceTint routing. The sRGB transfer for the role was made explicit in
  `a0f75fc56`; before that, every DDS was uploaded as `*_SRGB` anyway.
- **Description**:
  - **The shader's stated invariant.** The comment says the modulation is
    centred on 1.0 "so a 0.5 grey detail sample is a no-op".
  - **The upload breaks it.** The texture is uploaded with an sRGB view, so a
    texel is linearised before the ×2. The identity point therefore sits at a
    *linear* 0.5, which is encoded ≈188/255, not 128/255.
  - **The vanilla content breaks it further.** Skyrim's neutral detail texture is
    `blankdetailmap.dds`. By name it is the texture an NPC with no complexion
    detail gets, and it is a uniform (65, 64, 65)/255.
  - **Result.** Its linear value is ≈0.053, so the shader multiplies the face
    albedo by ≈0.106. The authored complexion maps (`maleheaddetail_*`,
    `femaleheaddetail_*`) average the same ≈0.25 encoded, so no vanilla face
    escapes the darkening.
  - **The ×2 was never right for this role, even ignoring the decode.** Applied
    in encoded space, it would still halve the face (2 × 0.255 = 0.51). Skyrim's
    FaceTint detail combine therefore does not have its neutral point at 0.5.
- **Evidence** (census, `Skyrim - Meshes0.bsa` + `Skyrim - Meshes1.bsa`, every
  imported shape; textures from `Skyrim - Textures0..8.bsa`, fully BC1-decoded):
  - 78,146 imported shapes, of which 3,149 carry a `detail` role across 18 distinct
    paths, and 0 carry `dark`.
  - Every one of the 3,149 is `material_kind == 4`, inside
    `meshes\actors\character\facegendata\facegeom\`.
  - `textures\actors\character\male\blankdetailmap.dds` (256², DXT1):
    **65,536 of 65,536 texels = (65, 64, 65)**. It is referenced by 1,616 shapes,
    plus the Argonian and Khajiit copies (77 more), which are byte-identical.
  - `maleheaddetail_rough01.dds` (512², DXT1): mean (63.9, 61.9, 62.6)/255, and
    145,548 of 262,144 texels are exactly (65, 64, 65).
  - Shape-weighted encoded-luma histogram of all 3,149 detail textures: 3,140 in
    [0.2, 0.3) and 9 in [0.3, 0.4).
  - **Resulting shader multiplier** (`2 × srgb_decode(mean)`): 0.080–0.157. It is
    0.106 for the blank map.
  - Oblivion (9,545 NIFs, 41,132 shapes), FO3 (10,989 / 32,118) and FNV
    (14,881 / 48,982): **0** `detail` roles.
- **Impact**:
  - Every vanilla Skyrim NPC face (FaceGeom head) renders at about a tenth of its
    diffuse before lighting, on both the raster path and RT reflections/GI.
  - With D7-02 applied on top, the result is near-black.
  - The `DBG_BYPASS_DETAIL` flag (`BYROREDUX_RENDER_DEBUG=0x2`) removes this
    factor on the primary path only.
- **Related**: #399, #2694, REN-2026-09-16-D7-02, REN-2026-09-16-D6-01,
  NIFAL-D8-2026-09-16-05 (the colour-space test pins `detail` as sRGB, which
  freezes this choice).
- **Suggested Fix**:
  1. Do not pick a new constant by eye. First find a source for Skyrim's
     FaceTint detail combine: its scale, its neutral value, and whether it
     operates in encoded space.
  2. Encode that combine at the NIFAL boundary, as a canonical detail-blend
     parameter or a distinct role, rather than as a Skyrim branch in the shader.
  3. Whatever is chosen, pin it with a CPU-side test asserting that the measured
     vanilla neutral (65/255) maps to an albedo multiplier of 1.0 under the
     chosen transfer function and scale.

#### REN-2026-09-16-D7-02: The tint role multiplies albedo by the `_sk.dds` RGB with weight = its alpha, and vanilla `_sk` maps are alpha-less DXT1, so Skyrim skin albedo is scaled by (0.09, 0.02, 0.02)
- **Severity**: HIGH. Skin colour is wrong on every vanilla Skyrim actor head,
  body and hands, on both primary and secondary rays.
- **Dimension**: Material Table (texture-role consumption)
- **Game Affected**: Skyrim (live on 4,100 vanilla shapes). FO4 and FO76 tint
  families route the same role (`slot_to_role` slot 2); their `_sk` content was
  not measured in this sweep.
- **Location**:
  - `crates/renderer/shaders/triangle.frag`: the `mat.tintMapIndex` block,
    `albedo = mix(albedo, albedo * tintSample.rgb, tintSample.a)`.
  - The same line is in `rayHitAlbedo` (`crates/renderer/shaders/include/ray_hit.glsl`).
  - The colour space is `tint: slot(&textures.tint, srgb)` in
    `map_secondary_texture_handles`.
  - The producer is the tint-family arm of `slot_to_role`
    (`crates/nif/src/import/material/slot_role.rs`), which documents the role as
    "Skin- or hair-tint mask (`*_sk.dds`)".
- **Status**: NEW. The consumer was added in `1d94eb246` (the unification commit
  this preset follows up) without a recorded measurement of `_sk` semantics.
  #2694 later widened the producer to FaceTint.
- **Description**:
  - **How the shader reads the role.** It treats `tint.rgb` as a multiplicative
    albedo colour and `tint.a` as the blend weight.
  - **What vanilla content supplies.** All nine distinct vanilla `_sk` paths
    reached through this role are DXT1. None of the decoded head, body and hand
    maps below contains a punch-through (alpha 0) texel. The sampled alpha is
    therefore 1.0 on every texel, and the full multiply always applies.
  - **What the RGB contains.** The RGB is a dark, saturated red. Whatever `_sk`
    encodes (NIFAL calls it a mask; the modding community commonly describes it
    as the skin subsurface/tone map, which is not verified against a primary
    source here), it is not an albedo multiplier whose neutral value is 1.0.
  - **Effect of the sRGB decode.** It pushes the multiplier down by another
    factor of 3–5.
- **Evidence** (same census and archives as D7-01; 4,100 shapes with a `tint`
  role across 9 paths, all DXT1, 0 missing). Full BC1 decode with alpha, reported
  as the effective per-channel multiplier
  `mean((1 − a) + a · srgb_decode(rgb))`:

  | texture | shapes | encoded mean | albedo multiplier |
  |---|---|---|---|
  | `male\malehead_sk.dds` | 2,110 | (0.309, 0.147, 0.127) | **(0.086, 0.021, 0.016)** |
  | `female\femalehead_sk.dds` | 1,078 | (0.506, 0.300, 0.271) | ≈(0.22, 0.07, 0.06) (from mean) |
  | `male\malebody_1_sk.dds` | 329 | (0.279, 0.137, 0.119) | (0.064, 0.017, 0.013) |
  | `male\malehands_1_sk.dds` | 123 | (0.291, 0.138, 0.119) | (0.069, 0.017, 0.013) |
  | `female\femalehands_1_sk.dds` | 119 | (0.542, 0.356, 0.319) | (0.256, 0.115, 0.089) |
  | `malechild\*_sk.dds` | 75 | (0.063, 0.016, 0.031) | (0.005, 0.001, 0.002) |

  Alpha-0 texel fraction on every decoded map: 0.000.
- **Impact**:
  - Skyrim skin renders as a dark red-brown at between 1/5 and 1/200 of its
    diffuse, depending on channel and texture.
  - On FaceGeom heads it stacks with D7-01, taking a male face to about
    (0.009, 0.002, 0.002) × diffuse before lighting.
  - GI bounce and reflections of actors inherit the same colour through
    `rayHitAlbedo`.
  - No debug flag bypasses this term.
- **Related**: `1d94eb246`, #2694, #1350, #3458, REN-2026-09-16-D7-01
- **Suggested Fix**:
  1. Establish `_sk` semantics from a source before changing the math.
  2. If `_sk` is a subsurface or skin-tone input rather than an albedo multiplier,
     route it at the NIFAL boundary to the role or parameter that consumes it
     (for example the translucency/subsurface colour), and stop multiplying it
     into albedo.
  3. If a multiplicative use is confirmed, the weight cannot come from DXT1 alpha.
     The consumer also needs the same "vanilla neutral maps to 1.0" pin proposed
     in D7-01.

#### REN-2026-09-16-D6-01: `select_facegen_diffuse` applies the per-NPC FaceGen tint DDS to `material_kind == 5` shapes, but every vanilla Skyrim FaceGeom head is kind 4 — the face atlas lands on scars, Argonian hair and Orc tusks, and the head keeps the generic race diffuse
- **Severity**: HIGH. The wrong base-colour texture reaches the canonical
  `Material` and `TextureHandle` on every vanilla Skyrim NPC that has pre-baked
  FaceGen.
- **Dimension**: NIFAL Material (base-colour role selection feeding `translate_material`)
- **Game Affected**: Skyrim (measured). FO4, FO76 and Starfield share the
  pre-baked path (`prebaked_facegen_tint_path`); their head `material_kind` was not
  measured in this sweep.
- **Location**:
  - The gate: `select_facegen_diffuse` and `MATERIAL_KIND_SKIN_TINT`
    (`byroredux/src/scene/nif_loader.rs`).
  - The call site in the same file:
    `owned_textures.base_color = select_facegen_diffuse(..., mesh.material.material_kind)`.
  - The test that encodes the premise: `facegen_diffuse_override_targets_only_skin_tint_head`.
  - The caller: `PrebakedPhase::Facegen` in `byroredux/src/npc_spawn/resumable.rs`,
    which passes the result of `prebaked_facegen_tint_path` as the diffuse override.
- **Status**: Regression of #2095 (closed). The gate was introduced in
  `b3a53e567` (2026-08-09) to stop the override replacing mouth, eye and hair
  textures. It keyed on SkinTint (5) on the stated premise that the head mesh is
  the SkinTint mesh. Skyrim's FaceGeom head uses FaceTint (4), and
  `canonical_shader_type` does not remap 4 on the Skyrim layout.
- **Description**:
  - **The doc comment's claim.** It says the generated FaceTint DDS "is the
    diffuse replacement for the SkinTint head mesh only".
  - **What vanilla FaceGeom NIFs contain.** The head mesh is `material_kind 4`.
    The only kind-5 shapes are overlays with their own authored textures: facial
    scars, Argonian hair and Orc tusks.
  - **Result.** The override is never applied to any vanilla head. Instead, the
    per-NPC face atlas is bound as the diffuse of those overlay meshes, which is
    exactly the layered "mush" `b3a53e567` set out to prevent.
  - **Why the test did not catch it.** The unit test hard-codes the premise
    `(MATERIAL_KIND_SKIN_TINT, "head.dds", FACE_TINT)`, so it passes.
- **Evidence** (census over `Skyrim - Meshes0.bsa` + `Skyrim - Meshes1.bsa`,
  `ImportedMaterial.material_kind` from `import_nif`; "FaceGeom" means the path
  contains `facegendata`):
  - `kind=4, facegeom, detail+tint, base texture contains "head"`: **3,149**
    (`malehead.dds`, `femalehead.dds`, `maleheadvampire.dds`,
    `argonianfemalehead.dds`, …), plus one more kind-4 FaceGeom head without
    `detail`.
  - `kind=5, facegeom`: **750** in total, none with a head base texture. Their base textures include
    `actors\character\male\facedetails\faceleftsidegash*.dds`,
    `…\female\facedetails\facefemaleleftsidegash_*.dds`,
    `textures\actors\character\argonianmale\argonianhair.dds`,
    `…\argonianfemale\argonianfemalehair.dds` and
    `textures\actors\character\orcmale\orctusks.dds`.
  - Code: `if material_kind == MATERIAL_KIND_SKIN_TINT { diffuse_override… }`,
    with `MATERIAL_KIND_SKIN_TINT: u32 = 5`.
- **Impact**:
  - Every Skyrim NPC with pre-baked FaceGen shows the generic race head texture:
    no per-NPC skin tone, complexion or make-up from its FaceTint DDS.
  - Any NPC whose FaceGeom includes a scar, Argonian hair or Orc tusks shows
    pieces of the face atlas on those overlays.
  - This is independent of D7-01 and D7-02, which darken whatever diffuse the
    head ends up with.
- **Related**: #2095, `b3a53e567`, #2694 (which established that FaceTint belongs
  to the tint family), REN-2026-09-16-D7-01, REN-2026-09-16-D7-02
- **Suggested Fix**:
  1. Key the override on the FaceGen head shape, measured, rather than on
     SkinTint: `material_kind == FACE_TINT` on the Skyrim layout, or better, a
     canonical "FaceGen head" flag set at the NIFAL boundary, where the per-game
     shader-type numbering is already normalised.
  2. Replace the unit test's hand-written `(5, "head.dds")` case with the measured
     `(4, "malehead.dds")` head and the `(5, "faceleftsidegash04.dds")` overlay.
  3. Confirm the FO4, FO76 and Starfield head kinds before widening the gate.

### LOW

#### REN-2026-09-16-D7-03: #4201 made `material_hash` build the struct, but the two call sites still route through `intern_by_hash` with a second build and comments saying the build is skipped; three doc sites still describe the retired field walk
- **Severity**: LOW. The code is correct; the prose is stale, and a miss does
  redundant work.
- **Dimension**: Material Table
- **Location**:
  - The call sites: `collect_static_mesh_draws` (`byroredux/src/render/static_meshes.rs`)
    and `emit_particles` (`byroredux/src/render/particles.rs`), both
    `intern_by_hash(cmd.material_hash(), || cmd.to_gpu_material())`.
  - The comment at the static-mesh site: "`intern_by_hash` skips the
    `to_gpu_material()` construction on the dedup-hit path".
  - The `MaterialTable::intern_by_hash` doc in `crates/renderer/src/vulkan/material.rs`:
    "`to_gpu_material` (the dominant construction cost) is skipped on the ~97%
    dedup-hit path", and "a pure function of the same fields … in the same order".
  - The `GpuMaterial` doc: "the byte-level `Hash`/`Eq` impls below"
    (`GpuMaterial` has no `Hash` impl).
  - The `byroredux/src/cornell.rs` doc on the probe `material_alpha`:
    "material.rs writes `mat.material_alpha.to_bits()`".
- **Status**: NEW (a side effect of `479ce5266` / #4201, today)
- **Description**:
  - **What changed.** #4201 redefined `DrawCommand::material_hash` as
    `hash_gpu_material_fields(&self.to_gpu_material())`, and its own doc says so.
  - **What that leaves at the call sites.** On a dedup miss the struct is now
    built twice: once for the hash, once in the factory closure. In debug builds
    it is built twice on every hit too, because of the collision check. The
    closure-on-miss API exists only to skip a build that no longer gets skipped.
  - **Stale descriptions.** The listed comments still describe that skip, or the
    per-field `to_bits()` walk #4201 removed.
- **Evidence**: `pub fn material_hash(&self) -> u64 { super::super::material::hash_gpu_material_fields(&self.to_gpu_material()) }`
  in `crates/renderer/src/vulkan/context/types.rs`; the two call sites above.
- **Impact**:
  - The redundant build on the roughly 3% miss path is small but free to remove.
  - Readers are told a performance property that no longer exists, and are told
    to keep a field order that no longer matters.
- **Related**: #4201, #781, #3568
- **Suggested Fix**:
  1. At both call sites, call `material_table.intern(cmd.to_gpu_material())`
     (one build, one hash). Alternatively, keep `intern_by_hash` but hash a
     locally built struct and move it into the closure.
  2. Rewrite the four comments to describe the byte-hash.

#### REN-2026-09-16-D7-04: `GpuMaterial`'s per-group offset comments (Rust and GLSL) and three doc sentences still describe the pre-#3909 layout and the pre-growth role count
- **Severity**: LOW (documentation; the offset pins are the real guard and they pass)
- **Dimension**: GPU-Struct Layout / Material Table
- **Location**:
  - `crates/renderer/src/vulkan/material.rs`: the group banners inside
    `struct GpuMaterial`, the supplemental-role banner, and `mod material_flag`.
  - `crates/renderer/shaders/include/bindings.glsl`: `struct GpuMaterial`.
  - `docs/engine/shader-pipeline.md`: the `GpuMaterial` section.
- **Status**: NEW
- **Description**: #3909 removed `texture_index` at offset 48 and shifted every
  later field down by 4. The per-field `// offset N` comments were updated; the
  group banners were not.
  - **Rust banners.** They still read "skin_tint … (offsets 144-156)" for fields
    at 140–152, "(offsets 160-172)", "(176-188)", "(192-204)", "(208-220)",
    "(224-236)", "(240-256)", "vec4 #18; offsets 260-280", "vec4 #19; offsets
    276-280", "vec4 #20; offsets 280-292" for `ior` at 276, "vec4 #21; offsets
    284-296", "(offsets 300-344)" for roles at 296–340, "(offsets 348-360)",
    "(offsets 364-392)" and "(offsets 396-428)".
  - **GLSL mirror.** It says "Offset 280" for `ior` (276) and still carries
    "(208-220)", "(224-236)" and "(240-252)".
  - **Role counts.** `crates/renderer/src/vulkan/material.rs` says "Nine of the twelve are sampled". The
    group now holds 16 roles, 13 of them sampled. `docs/engine/shader-pipeline.md` says "The
    twelve entries at 300–344" (296–340), and "included in both draw-command and
    `GpuMaterial` hashing" (there is one byte-hash since #4201).
  - **Flag bits.** `material_flag::EFFECT_LI_SHIFT` says "Bits 11–15 are reserved
    for future single-bit flags", but bits 11, 12, 13, 14 and 15 are all assigned
    (`THIN_GLASS`, `MSN_HAS_AUTHORED_Z`, `SOFT_LIGHTING`, `RIM_LIGHTING`,
    `BACK_LIGHTING`).
  - **GLSL literals.** The `EFFECT_*` block says "the GLSL refers to the same
    `0x...u` literals". The GLSL includes the generated
    `crates/renderer/shaders/include/shader_constants.glsl` instead, and the
    module's own header forbids hand-written literals.
- **Evidence**: `// offset 140` on `skin_tint_a` directly under the "offsets
  144-156" banner; `float ior;` under "Offset 280."; `pub const THIN_GLASS: u32 = 1 << 11;`.
- **Impact**: A contributor adding a field would read offsets that are 4 bytes
  wrong, in the one struct whose layout the skill calls the load-bearing lockstep
  contract.
- **Related**: #3909, #806, #4201, REN-2026-09-05-D7-02
- **Suggested Fix**:
  1. Drop the numeric ranges from the group banners (the per-field comments and
     the pin test already carry them), or regenerate them.
  2. Update the three prose sentences and the `EFFECT_LI_SHIFT` / `EFFECT_*` notes.

#### REN-2026-09-16-D6-02: The parallax and IOR neutral defaults are still restated as literals at five production sites after #3073/#3912 named them
- **Severity**: LOW (the values currently agree)
- **Dimension**: NIFAL Material / Material Table
- **Location**:
  - `GpuMaterial::default` (`crates/renderer/src/vulkan/material.rs`):
    `parallax_height_scale: 0.04`, `parallax_max_passes: 4.0`. It already
    imports `DEFAULT_DIELECTRIC_IOR` from the same module.
  - `emit_particles` (`byroredux/src/render/particles.rs`): `0.04`, `4.0` and
    `ior: 1.5`. The same literal block already uses
    `DEFAULT_GLASS_REFRACTION_SCALE` / `DEFAULT_GLASS_BLUR_SCALE` (#3912).
  - The `MaterialTextureHandles` literals in `byroredux/src/cell_loader/terrain.rs`
    and `byroredux/src/cell_loader/terrain_lod.rs`: `0.04`, `4.0`.
- **Status**: NEW. #3073 introduced `DEFAULT_PARALLAX_HEIGHT_SCALE` /
  `DEFAULT_PARALLAX_MAX_PASSES` and #3912 applied the same doctrine to the glass
  scalars, but these sites were not converted.
- **Description**: `collect_static_mesh_draws` uses the named constants
  (`crates/core/src/ecs/components/material.rs`) for its no-`MaterialTextureHandles`
  fallback. #3912 was fixed on the stated rule that "a canonical retune can't leave
  the no-material fallback on the old number". The remaining literal copies are
  exactly that hazard for the parallax pair and the particle IOR.
- **Evidence**: `pub const DEFAULT_PARALLAX_HEIGHT_SCALE: f32 = 0.04;` against the
  literals above. `grep -rn "parallax_height_scale: 0.04"` finds the production
  hits listed; the remaining hits are tests, the Cornell harness and a console
  test helper.
- **Impact**: None today. A retune of the canonical default would silently leave
  the neutral material slot 0, particles and near/LOD terrain on the old value.
- **Related**: #3073, #3912, REN-2026-09-05-D6-02
- **Suggested Fix**: Replace the literals with the three named constants.

#### REN-2026-09-16-D7-05: `GpuMaterial` has three independent `unsafe` byte views, and #4201 made the one with a prose-only safety argument the dedup identity
- **Severity**: LOW (hardening / duplication; every view is currently sound)
- **Dimension**: Material Table / GPU-Struct Layout
- **Location**:
  - `GpuMaterial::as_bytes` (`crates/renderer/src/vulkan/material.rs`): used by
    `PartialEq` and, since #4201, by `hash_gpu_material_fields`.
  - `hash_material_slice` (`crates/renderer/src/vulkan/scene_buffer/descriptors.rs`).
  - `byte_view<T: NoUninit>` (`crates/renderer/src/vulkan/buffer.rs`): private,
    and used by `write_mapped`.
- **Status**: NEW
- **Description**: #3990 introduced `unsafe trait NoUninit` so that the
  "no uninitialised bytes" argument lives at the type level, and `GpuMaterial`
  implements it. The two material-specific views still carry their own `unsafe`
  blocks with prose justifications instead of reusing the bounded helper.
  - `as_bytes`' prose says "all padding bytes are named fields the producer always
    initialises".
  - `hash_material_slice` says "explicit padding fields". `GpuMaterial` has had no
    pad fields since #3909.

  Both still hold, because every field is a 4-byte scalar. They are the kind of
  argument #3990 and #3761 set out to retire, and `as_bytes` is now the key that
  decides material identity.
- **Evidence**:
  - `unsafe { std::slice::from_raw_parts(self as *const Self as *const u8, …) }`
    in `as_bytes`.
  - The same pattern over a slice in `hash_material_slice`.
  - `fn byte_view<T: NoUninit>(data: &[T]) -> &[u8]` (not `pub(crate)`).
- **Impact**: None today. A future `GpuMaterial` field that introduces padding
  would be caught by `NoUninit`'s audit point only at `write_mapped`, not at the
  dedup hash or equality.
- **Related**: #3990, #3761, #4201
- **Suggested Fix**: Make `byte_view` `pub(crate)` and implement `as_bytes` and
  `hash_material_slice` on top of it. This removes two `unsafe` blocks, and the
  per-user rule favours improving the shared helper over keeping copies.

## Prioritized Fix Order

1. **D6-01**: a gate-key correction plus a test rewrite. It is independent of the
   shading questions and restores per-NPC faces on the pre-baked path.
2. **D7-02, then D7-01**: both need a sourced answer for Skyrim's FaceTint/SkinTint
   `_sk` and detail combines before any arithmetic changes. Land each with a
   "measured vanilla neutral maps to 1.0" CPU-side pin, and re-check against
   NIFAL D8-05's colour-space pins.
3. **D7-03**: a two-line call-site simplification plus prose.
4. **D6-02, D7-05**: constant and helper consolidation.
5. **D7-04**: documentation.

## Needs visual confirmation (engine not launched in this sweep)

- **D7-01** can be A/B'd on a Skyrim interior with NPCs using
  `BYROREDUX_RENDER_DEBUG=0x2` (`DBG_BYPASS_DETAIL`). Faces should brighten by
  about 9× on the primary path. The secondary-ray path does not honour the flag.
- **D7-02** has no bypass flag, so an A/B needs a temporary build or `mat.set`
  support for `tint_map_index`.
- **D6-01**: compare a named NPC's head against its
  `facetint\skyrim.esm\<formid>.dds`, and inspect a scarred NPC's scar overlay for
  face-atlas content.
- Per the standing guidance, none of these is a Vulkan-state change. All three
  are CPU- or shader-arithmetic fixes whose visual result is the thing to verify.

## Regression guards verified (do not re-file)

- **Single boundary (Dim 6).**
  - `every_exterior_spawner_inserts_a_boundary_material` is present and green
    (#4302 walk still in place).
  - `translate_texture_only_material` is used by the terrain, LOD, object-LOD and
    water spawners.
  - The Cornell harness is still the documented literal exemption. #4304 (ground
    cover) is still open.
- **`resolve_pbr`.** It is run once at translate and pinned by
  `resolve_pbr_is_idempotent`. The render path reads `m.roughness` /
  `m.metalness` directly, and `collect_static_mesh_draws` has no `classify_pbr`.
- **No per-game branch** between `Material` and `MaterialTable::intern`. The
  variant packs key on canonical `material_kind` (5/6/11/14/16 after
  `canonical_shader_type`).
- **Particle slice.**
  - `apply_emitter_overlays` / `apply_emitter_params` and
    `apply_emitter_params_overrides_kinematics_and_size_not_color` are present.
  - `emit_particles` forwards `effect_shader_flags` (#2610) and
    `greyscale_lut_index` (#3590) verbatim.
  - `quantize_fade` is applied to the colour fade only (#1795), and to the
    animated sinks only, never to the static fallback (#3246).
- **Supplemental lanes.** `supplemental_texture_slot` has 16 entries and is
  written by index (#2697). The write is pinned by
  `every_supplemental_texture_slot_is_written_exactly_once` (#3814).
- **The #3909 removal holds.** No `textureIndex` is read from `mat` in any shader.
- **Flipbook normal-alpha gate (#4301).** `apply_texture_flip_roles` returns the
  active frame's alpha presence, pinned by
  `a_normal_flip_uses_the_active_frames_alpha_presence`.
- **Parallax gates.** `PARALLAX_ALPHA_HEIGHT_BIT` is gated on `normal_has_alpha`
  (#3562). When there is no alpha, the slot is zeroed (#4260).
- **Mesh-water indices.** `water_material_from_mesh` translates handle 0 to the
  water shader's `u32::MAX` "none" sentinel for both the normal and flow indices,
  so the per-role resolver's 0 cannot reach `crates/renderer/shaders/water.frag`'s `sampleFlowMap` as the
  checker texture.
- **Tests executed** (all green):
  - `cargo test -p byroredux-renderer --lib -- material hash_gpu_material`: 65 passed.
  - `cargo test -p byroredux --bin byroredux -- facegen static_meshes every_exterior_spawner common_material_texture_walk`:
    37 passed, 3 ignored. Note that `facegen_diffuse_override_targets_only_skin_tint_head`
    passes on the wrong premise (D6-01).

## Pre-existing open issues confirmed still present (not re-filed)

- **#4304** (ground cover has no canonical `Material`): unchanged.
- **#4256** (`shader_type` not on `Material`): still open. It is related to
  D6-01's suggested canonical FaceGen-head flag.
- **#4400** (MSWP glass-overlay roles): unchanged per NIFAL 09-16.

## Stale skill premises (for the next `/audit-renderer` sync)

- **Dim 3 / Dim 7 hash premise.** The skill says "The dedup hash now walks 107
  scalar fields (`hash_gpu_material_fields`, #1368) … re-count with `awk`", and
  that "The dedup key is the field-walking `hash_gpu_material_fields`". Since
  `479ce5266` (#4201, 2026-09-16) the hash is a single `FxHasher::write` over
  `GpuMaterial::as_bytes`. There is no walk to count, and
  `DrawCommand::material_hash` is equal to it by construction. The coverage guard
  is now `hash_gpu_material_fields_covers_every_gpu_material_field` (per-byte XOR
  probe).
- **Dim 7 identity bullet.** "`GpuMaterial` has no `Hash` impl" is still true, but
  "`DrawCommand::material_hash` must stay in lockstep with it" is now automatic.
  `material_hash_matches_gpu_material_field_hash` survives only as a guard
  against a future second walk.
- **Suggested addition to Dim 7.** Add a "consumer neutral-value" check for
  multiplicative texture roles (`detail`, `tint`, `dark`). The per-role
  colour-space table and the shader arithmetic are only correct together, and
  neither the skill nor any test ties them.

## Method notes

- **Census** (release build, scratchpad crate, not committed):
  1. Every `.nif` in Oblivion (`Oblivion - Meshes.bsa`,
     `DLCShiveringIsles - Meshes.bsa`, `Knights.bsa`), FO3
     (`Fallout - Meshes.bsa`), FNV (`Fallout - Meshes.bsa`) and Skyrim SE
     (`Skyrim - Meshes0.bsa`, `Skyrim - Meshes1.bsa`) was imported with
     `byroredux_nif::import::import_nif`.
  2. Shapes carrying a `detail` or `tint` role were counted, and their paths were
     canonicalised to `textures\…`.
  3. Each distinct texture was extracted from the game's texture BSAs and every
     BC1 block was decoded, including the 3-colour punch-through mode.
  4. A second pass histogrammed `(material_kind, is_facegeom, has_detail,
     has_tint, base texture)` for kind 4/5 shapes.
- **Dedup**:
  - `gh issue list --limit 200` (open).
  - Full-state searches for `tint`, `detail map`, and
    `skin dark OR black face OR facetint OR _sk`.
  - Prior reports: `AUDIT_RENDERER_2026-09-14`, `AUDIT_RENDERER_2026-09-05_DIM6_DIM7`,
    and today's `AUDIT_NIFAL_2026-09-16` and `AUDIT_FO4_2026-09-16`.

  No open or closed issue covers D6-01 or D7-01..05. #2095 is closed, so D6-01 is
  reported as its regression.
- **Validate gate**: not run as a pass/fail signal for this report. Per the preset
  note, it currently fails on stale *crates/nif/src/blocks/shader.rs* references
  in other skills. Every backticked path in this report was checked against the
  live tree.

**Suggested publish labels**:
- **D6-01**: `high`, `bug`, `renderer`, `nifal`, `game:skyrim`.
- **D7-01**: `high`, `bug`, `renderer`, `shaders`, `game:skyrim`.
- **D7-02**: `high`, `bug`, `renderer`, `shaders`, `game:skyrim`.
- **D7-03**: `low`, `tech-debt`, `renderer`, `doc-rot`.
- **D7-04**: `low`, `documentation`, `renderer`, `shaders`, `doc-rot`.
- **D6-02**: `low`, `tech-debt`, `renderer`, `nifal`.
- **D7-05**: `low`, `tech-debt`, `renderer`, `safety`.

## Next Step

```
/audit-publish docs/audits/AUDIT_RENDERER_2026-09-16.md
```
