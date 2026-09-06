# Renderer Audit — 2026-09-05 (NARROWED: Dimensions 6 + 7 only)

> **⚠ This is a NARROWED run, not a full renderer audit.** It covers exactly
> two of `/audit-renderer`'s 23 dimensions, selected by the
> `texture-roles-deep` audit-suite preset (`/audit-renderer --focus 6,7`):
>
> | Dim | Name | Why it is in scope |
> |---|---|---|
> | 6 | NIFAL material canonical translation | the canonical `Material` and its texture roles are the thing being consumed |
> | 7 | Material table (R1 dedup) | `MaterialTable::intern` → `GpuMaterial` → descriptor sets → shaders is the consumption path |
>
> **Dimensions 1–5 and 8–23 were NOT examined.** Acceleration structures (1),
> SSBO/ray queries (2), GPU-struct layout (3), sync/barriers (4), GPU memory
> and lifecycle (5), denoiser/composite (8), skinning (9), camera-relative
> precision (10), pipeline/render pass (11), command recording (12), TAA (13),
> caustics (14), water (15), volumetrics/bloom (16), Disney BSDF and soft
> shadows (17), sky/weather (18), tangent space (19), telemetry (20), the
> Cornell harness (21), light animation (22) and the FSR/presentation chain
> (23) carry **no coverage** from this report.
>
> **Report path note:** `docs/audits/AUDIT_RENDERER_2026-09-05.md` was already
> taken by a *committed* report from the same day's `volumetrics-deep` preset
> (commit `fa5c4191`, dimensions 1/2/5/16). This report uses the repository's
> existing same-day suffix convention rather than overwriting it. The two
> reports are disjoint in scope; neither supersedes the other.

**Target of this run.** The 2026-07-27 cross-game texture-role unification —
`1d94eb24`, `05d68926`, `c8c8a834` — which introduced `MaterialTextureSet`,
folded `ImportedMaterial` into `ImportedMesh.material`, narrowed
*merge_bgsm_into_mesh* into `merge_external_material`, and grew `GpuMaterial`
from 300 B to (then) 348 B. The audit angle is deliberately the **consumption**
half: how the canonical material and its roles reach the material table,
`GpuMaterial`, the descriptor sets and the shaders. Texture roles are the new
per-game seam, so a mis-mapping is invisible in one game and wrong in another
— which is the shape every finding below was tested for.

**Verification discipline.** Every finding is anchored on a symbol confirmed by
`grep` against the live tree, not on a line number. Backticked `.ext` paths and
`snake_case`/`camelCase` symbols resolve now; a deliberately-absent symbol is
*italicised*. No render-pass / pipeline / barrier edit is proposed on reasoning
alone. Gamebryo colours are raw monitor-space floats — no missing
`srgb_to_linear` is reported as a bug. Legacy FO4 metalness is spec-colour
chromaticity, not luminance — untouched here.

**Dedup baseline.** `gh issue list --limit 400 --state all`
(`/tmp/audit/renderer/issues.json`, 400 issues). `docs/audits/` scanned for
prior renderer reports (most recent full sweep `AUDIT_RENDERER_2026-08-30.md`;
most recent same-day scoped `AUDIT_RENDERER_2026-09-05.md`; prior Dim-6/7
reports `AUDIT_RENDERER_2026-05-24_DIM6_14.md`,
`AUDIT_RENDERER_2026-06-28_DIM3_DIM6.md`, `AUDIT_RENDERER_2026-05-03_R1.md`).

---

## Executive Summary

| Severity | Count |
|---|---|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 1 |
| LOW | 6 |
| **Total** | **7** |

By dimension: Dim 6 → 3 LOW; Dim 7 → 1 MEDIUM + 3 LOW.

**The role plumbing itself is in good shape.** All 26 `MaterialTextureSet`
slots (22 named roles + `decals: [T; 4]`) reach `GpuMaterial` — ten through
dedicated fields, sixteen through the `supplemental_texture_slot` side-array —
and the CPU→GPU→GLSL chain is guarded at four independent points: an exhaustive
struct literal at role→bindless resolution, an arity pin on the supplemental
write block, a name+type+order cross-check between the Rust and GLSL
`GpuMaterial`, and a non-vacuous hash-lockstep pin. The REFR spawn path seeds
its role set with `map_ref` so roles with no explicit override arm carry
through by construction rather than being dropped. No per-game branch survives
between the canonical `Material` and `MaterialTable::intern`.

**The one substantive gap is on the RT half of consumption.** The shared
secondary-ray hit shader applies only the constant diffuse tint; five
albedo-modifying texture roles that the raster path composes — decals, tint,
inner layer, dark, detail — are absent from it, and those five are drawn from
three different game eras, so the divergence is per-game in exactly the way
this audit's angle predicts.

The remaining six findings are doc-rot, duplicated constants and
test-narrowing: two code comments still describing seven now-wired lanes as
unwired; two stale NIFAL facts in the audit skill files themselves; an
undocumented sixteenth unsampled `GpuMaterial` lane that sits in the dedup key;
two guards whose coverage silently narrowed as the role set grew; and three
copies of two canonical glass defaults that bypass the named constants.

**Already-open, not re-reported:** #3846 (`include/bindings.glsl` documents
`GpuMaterial` as 396 B and points the struct-sync invariant at
*gpu_material_size_is_396_bytes*, a test that has never existed; live struct is
432 B / `gpu_material_size_is_432_bytes`). Confirmed still present in the live
file during this run — the premise holds, the issue stands.

---

## Findings

### REN-2026-09-05-D7-01: secondary-ray albedo drops every albedo-modifying texture role

- **Severity**: MEDIUM
- **Dimension**: Material Table
- **Location**: `crates/renderer/shaders/include/ray_hit.glsl`
  (`rayHitAlbedo`, `sampleRayHitBase`, `rayHitHasCoverage`), against
  `crates/renderer/shaders/triangle.frag`
- **Status**: NEW
- **Description**: The shared hit-reconstruction helper every material-aware
  secondary ray goes through resolves surface colour as
  `max(baseRgb * vec3(mat.diffuseR, mat.diffuseG, mat.diffuseB), vec3(0.0))`,
  with `baseRgb` sampled from `textures[inst.textureIndex]` alone. The raster
  path composes albedo from five further canonical texture roles, none of which
  any secondary ray reads. The RT reflection, 1-bounce GI and water-refraction
  termini therefore shade a *different surface colour* than the raster pass
  shades for the same fragment.
- **Evidence**: `grep 'mat\.' crates/renderer/shaders/include/ray_hit.glsl`
  yields only `uvScaleU/V`, `uvOffsetU/V`, `parallaxMapIndex`,
  `parallaxHeightScale`, `parallaxMaxPasses`, `alphaThreshold`,
  `materialAlpha`, `alphaTestFunc`, `materialKind`, `diffuseR/G/B`,
  `glowMapIndex`, `emissiveR/G/B`, `emissiveMult`. Missing relative to
  `triangle.frag`:

  | Role → `GpuMaterial` field | Raster application | In `ray_hit.glsl`? |
  |---|---|---|
  | `decals[0..3]` → `decalMap0..3Index` | alpha-over into `texColor` **including `texColor.a`** | no |
  | `tint` → `tintMapIndex` | `mix(albedo, albedo * tint.rgb, tint.a)` | no |
  | `inner_layer` → `innerLayerMapIndex` | `mix(albedo, inner.rgb, thickness * inner.a)` | no |
  | `dark` → `darkMapIndex` | `albedo *= darkSample` | no |
  | `detail` → `detailMapIndex` | `albedo *= detailSample * 2.0` | no |

  The decal row is the strongest: decals rewrite `texColor.a`, and
  `rayHitHasCoverage` derives ray coverage from `baseSample.a`, so a
  decal-layered alpha-tested surface presents different coverage to shadow and
  GI rays than it does to the rasteriser.
- **Impact**: Per-game by construction, which is why it is easy to miss.
  `dark` is a Gamebryo/Oblivion-era `NiTexturingProperty` role — Oblivion and
  FO3/FNV interiors bounce light off walls without their baked shadow
  modulation, over-brightening GI. `tint` is the Skyrim/FO4 tint family's
  `*_sk.dds` on slot 2 — Skyrim and FO4 heads reflect and bounce untinted skin.
  `decals` and `detail` are the legacy overlay layers — the reflected image of
  a decalled surface is the undecalled base. Visual only; no crash or
  corruption path.
- **Related**: `AUDIT_RENDERER_2026-08-14.md` REN-D2-01 fixed a *terminus* that
  bypassed `rayHitAlbedo` entirely (it used `GpuInstance.avgAlbedo*`); this is
  the complementary gap inside `rayHitAlbedo` itself. The codebase already
  treats raster/ray agreement as an explicit invariant class:
  `water_fragment_uses_shared_material_aware_ray_hits` and the sibling alpha
  contract test in
  `crates/renderer/src/vulkan/scene_buffer/shader_contract_tests.rs` both
  assert it, and the latter states outright that "Raster and every
  material-aware ray must agree on the complete authored alpha expression".
  Note that `ray_hit.glsl` carries **no** note deferring these roles —
  contrast `triangle.frag`, which explicitly parks the EyeEnvmap and
  MultiLayerParallax-refraction variant stubs in prose right where a reader
  searching by `materialKind == N` will find them.
- **Suggested Fix**: Either extend `rayHitAlbedo` to apply the five roles (it
  already receives `mat` and `uv`, and `textureLod` with the caller's `lod` is
  the established pattern in this file), or — if the cost of five extra
  `textureLod` fetches per hit is judged unacceptable on the secondary-ray
  path — add the same kind of explicit deferral note `triangle.frag` uses for
  its variant stubs, plus a source-scan guard so the decision is recorded
  rather than inferred. Do not fix the decal half without also deciding what
  the coverage (`baseSample.a`) contract should be.

---

### REN-2026-09-05-D6-01: "captured, not yet shaded" is stale for seven now-wired canonical `Material` fields

- **Severity**: LOW
- **Dimension**: NIFAL Material
- **Location**: `crates/core/src/ecs/components/material.rs` (the doc block
  spanning `lighting_effect_1` … `fresnel_power`);
  `byroredux/src/material_translate.rs` (the comment above the
  `lighting_effect_1: source.lighting_effect_1,` copy block)
- **Status**: NEW
- **Description**: Two code sites still tell the reader that the Bethesda
  lighting-response scalars are captured on the canonical `Material` but have
  no GPU consumer. They gained one on 2026-08-25. The `crates/core` block says
  "Landed here (captured, not yet shaded) rather than also wiring a
  `GpuMaterial`/`triangle.frag` consumer in the same change", calls the group
  "captured, awaiting a `GpuMaterial`/shader consumer", and closes with "Both
  remain listed in `docs/engine/nifal.md`'s parked-passthrough inventory until
  that consumer lands." The `material_translate.rs` comment says "#2284
  (MAT-D1-NEW-04) — Skyrim+/FO4 BSLightingShaderProperty shading scalars.
  Captured, not yet shaded (no GpuMaterial / triangle.frag consumer)".
- **Evidence**: All seven fields are live end-to-end.
  `GpuMaterial.{lighting_effect_1, lighting_effect_2, subsurface_rolloff,
  rimlight_power, backlight_power, fresnel_power}` occupy offsets 396–416 and
  `grayscale_to_palette_scale` occupies 420
  (`crates/renderer/src/vulkan/material.rs`). They are consumed by
  `bethesdaDiffuseLightFactor`, `bethesdaRimFactor` and `bethesdaBackFactor`
  in `crates/renderer/shaders/include/lighting.glsl`, flag-gated on
  `MAT_FLAG_SOFT_LIGHTING` / `MAT_FLAG_RIM_LIGHTING` / `MAT_FLAG_BACK_LIGHTING`,
  and `grayscaleToPaletteScale` is applied as a bounded blend at two sites in
  `crates/renderer/shaders/triangle.frag`. `docs/engine/nifal.md` was updated
  and now reads "**The GPU-side follow-up closed on 2026-08-25:** all six are
  present in the 432-byte `GpuMaterial`…" — so the code comment points at a
  parked-passthrough inventory that no longer lists them.
- **Impact**: A reader auditing the NIFAL boundary is told seven populated
  lanes are dead. That justifies exactly the wrong maintenance decisions —
  deleting them as unused, or skipping them when reasoning about why a
  Skyrim/FO4 surface shades the way it does. This is the same failure class the
  project's own doc-rot precedent (`GpuMaterial` documented at 300 B after it
  reached 348 B) was raised over: a false premise an auditor can check and
  believe.
- **Related**: #2284, #2443, #2592, #3846 (the sibling stale size claim in
  `include/bindings.glsl`, already open).
- **Suggested Fix**: Rewrite both blocks to describe the shipped state and
  point at `crates/renderer/shaders/include/lighting.glsl` as the consumer;
  keep the #2592 historical correction, which is still accurate and still
  earns its place.

---

### REN-2026-09-05-D7-02: `GpuMaterial.texture_index` is a fourth unsampled lane — undocumented, and in the dedup key

- **Severity**: LOW
- **Dimension**: Material Table
- **Location**: `crates/renderer/src/vulkan/material.rs`
  (`GpuMaterial::texture_index`, `hash_gpu_material_fields`);
  `crates/renderer/shaders/include/bindings.glsl` (`textureIndex`);
  `crates/renderer/src/vulkan/context/mod.rs`
  (`DrawCommand::to_gpu_material`, `DrawCommand::material_hash`)
- **Status**: NEW
- **Description**: No shader reads `mat.textureIndex`. Both base-colour
  consumers take the handle from the *instance* record instead:
  `crates/renderer/shaders/triangle.vert` writes
  `fragTexIndex = inst.textureIndex` and `triangle.frag` samples through that
  varying; `crates/renderer/shaders/include/ray_hit.glsl`'s
  `sampleRayHitBase` samples `textures[inst.textureIndex]` directly. The
  `GpuMaterial` copy is nonetheless populated, mirrored in GLSL, uploaded, and
  hashed in **both** dedup walks.
- **Evidence**: `grep -rE '[A-Za-z]*[Mm]at[A-Za-z]*\.textureIndex'` across
  every `.vert`/`.frag`/`.comp` and `crates/renderer/shaders/include/*.glsl`
  returns zero hits; the only `.textureIndex` reads are `inst.`/`hInst.`/
  `tInst.`-qualified. On the Rust side the only non-test reader is
  `h.write_u32(mat.texture_index)` in `hash_gpu_material_fields`, mirrored by
  `h.write_u32(self.texture_handle)` in `DrawCommand::material_hash`.
- **Impact**: Two draws that differ *only* in base-colour texture get separate
  `GpuMaterial` entries even though the GPU would shade them identically from
  the material record. This is precisely the cost #2712 documented for
  `lighting_map_index` / `flow_map_index` / `wrinkle_map_index` ("a dedup-key
  lane … that can split two materials rendering byte-identically") — except
  base colour varies across draws far more than those three do, and the cap it
  pressures (`MAX_MATERIALS = 16384`) has a live overflow-to-id-0 path with its
  own counter surfaced by `ctx.scratch`. Unlike those three, this lane carries
  no deferral note on either the Rust field or the GLSL mirror, so a reader has
  no way to tell it is inert by design rather than by oversight.
- **Related**: #2712 (the deferral-documentation precedent), #797 (the
  over-cap warn + id-0 route), #780 (dedup-ratio telemetry).
- **Suggested Fix**: Decide and record which it is. If the lane stays, give it
  the same "#2712-shaped" note the other three carry and say explicitly that
  the base handle is per-instance by design. If it goes, dropping it from both
  hash walks (a behaviour change to the dedup key, not to any pixel) shrinks
  `GpuMaterial` and should be measured against the `ctx.scratch` dedup ratio
  before and after.

---

### REN-2026-09-05-D7-03: the #2712 shader-consumption guard now covers 9 of 13 sampled supplemental lanes

- **Severity**: LOW
- **Dimension**: Material Table
- **Location**: `crates/renderer/src/vulkan/material_tests.rs`
  (`supplemental_role_lanes_sampled_by_triangle_frag_are_exactly_the_nine`)
- **Status**: NEW
- **Description**: The guard asserts nine supplemental lane names appear in
  `triangle.frag`, and that three others (`lightingMapIndex`, `flowMapIndex`,
  `wrinkleMapIndex`) do not — the "if this pass stopped sampling it, the lane
  is now uploaded and hashed for nothing" contract. The supplemental set has
  since grown from twelve lanes to sixteen. Four of the new lanes are sampled
  and appear in neither list.
- **Evidence**: `supplemental_texture_slot::COUNT` is 16. Sampling status by
  `grep` over `crates/renderer/shaders/`:
  sampled by `triangle.frag` = `tintMapIndex`, `innerLayerMapIndex`,
  `specularMapIndex`, `reflectanceMapIndex`, `emittanceGradientMapIndex`,
  `decalMap0..3Index`, **`glassRoughnessScratchMapIndex`**,
  **`glassDirtOverlayMapIndex`**, **`lightingMaskMapIndex`**,
  **`backLightingMapIndex`** (13); sampled by no shader =
  `lightingMapIndex`, `wrinkleMapIndex` (2); `flowMapIndex` is deliberately
  material-table-unsampled — the name also occurs in
  `crates/renderer/shaders/water.frag`, but that is water's own push-constant
  lane (`push.uv_offset.z`, sentinel `0xFFFFFFFFu`), not the material record,
  so the deferral note is accurate. Total guarded: 9 + 3 = 12 of 16.
- **Impact**: The four bolded lanes are the newest and least-reviewed — the
  BGEM v21+ glass optics pair and the Bethesda soft/rim/back mask pair, both
  landed 2026-08-25. They are exactly the lanes most likely to lose a consumer
  in a shader refactor, and exactly the ones with no guard. Test coverage gap
  only; the code is correct today.
- **Related**: #2712, #3814.
- **Suggested Fix**: Move the four into the positive list and rename the test
  off the hard-coded count (the same rename discipline #3465 applied to the
  role-count prose). Better still, derive both lists from
  `supplemental_texture_slot`'s own doc enumeration so a seventeenth lane
  cannot be added without landing in one list or the other.

---

### REN-2026-09-05-D7-04: the supplemental write block is pinned for arity but not for role↔slot correspondence

- **Severity**: LOW
- **Dimension**: Material Table
- **Location**: `byroredux/src/render/static_meshes.rs`
  (`every_supplemental_texture_slot_is_written_exactly_once`, and the
  `supplemental_texture_indices[slot::…]` block it scans)
- **Status**: NEW
- **Description**: The guard source-scans the write block and asserts every
  declared `slot::` constant is assigned exactly once, and that no undeclared
  slot is written. It never inspects the right-hand side, so
  `supplemental_texture_indices[slot::TINT] = texture_indices.inner_layer;`
  passes. Because `DrawCommand::to_gpu_material` reads the array back through
  the *same* constants, a swapped pair of roles is symmetric: the wrong texture
  lands in the right GLSL field, with no compile error, no failing test, and no
  arity violation.
- **Evidence**: The test's assertions are `assigned.len() == declared.len()`,
  a per-name occurrence count of 1, and a membership check of `assigned`
  against `declared` — all on the `slot::X` token parsed out of
  `line.trim().strip_prefix("supplemental_texture_indices[slot::")`. The role
  expression after the `=` is never parsed.
- **Impact**: This is the exact failure shape the role unification was meant to
  make impossible, one layer down: a role mis-map is silent, and shows only on
  the games that author both roles of the swapped pair. Sixteen lanes, of which
  several pairs are plausibly confusable (`tint`/`lighting_mask` are colocated
  on Skyrim slot 2 by design; `specular`/`back_lighting` are colocated on
  slot 7; `glass_roughness_scratch`/`glass_dirt_overlay` are adjacent BGEM
  siblings). Test coverage gap only; the current sixteen assignments are
  correct.
- **Related**: #3814 (the issue this guard closed). #3814 deliberately scoped
  out a *different* question — which of the 26 canonical roles belong in the
  16-lane subset — as "a design decision, not a mechanical test", and asked for
  it to be documented at the declaration site instead. **That half was done**:
  `supplemental_texture_slot`'s doc comment now enumerates all sixteen role
  names in slot order and names the ten with dedicated fields. Which is what
  makes the correspondence pin mechanically available now in a way it was not
  when #3814 was written.
- **Suggested Fix**: Extend the existing source-scan to also capture the
  right-hand side (`texture_indices.<role>`) and check it against the role
  order the slot module's own doc comment declares — one more `split_once('=')`
  in a scan that already parses the line.

---

### REN-2026-09-05-D6-02: render-side glass defaults bypass the named canonical constants

- **Severity**: LOW
- **Dimension**: NIFAL Material
- **Location**: `byroredux/src/render/static_meshes.rs`,
  `byroredux/src/render/particles.rs`, `crates/renderer/src/vulkan/material.rs`
  (`GpuMaterial::default`)
- **Status**: NEW
- **Description**: `crates/core/src/ecs/components/material.rs` declares
  `DEFAULT_GLASS_REFRACTION_SCALE` (0.05) and `DEFAULT_GLASS_BLUR_SCALE` (0.4)
  and `Material::default()` uses them by name. Three downstream fallbacks
  hard-code the bare literals instead:
  `glass_refraction_scale: mat.map(|m| m.glass_refraction_scale).unwrap_or(0.05)`
  and `glass_blur_scale: … .unwrap_or(0.4)` in `static_meshes.rs`;
  `glass_refraction_scale: 0.05` / `glass_blur_scale: 0.4` in `particles.rs`;
  and the same pair in `GpuMaterial::default`.
- **Evidence**: `grep -rn 'DEFAULT_GLASS_REFRACTION_SCALE|DEFAULT_GLASS_BLUR_SCALE'
  byroredux/src` returns nothing — no render-side site imports either constant.
- **Impact**: Four independent copies of two canonical material defaults. A
  future retune of the canonical value silently leaves the no-`Material`
  fallback (LOD imposters, particles, terrain) and the GPU-side neutral default
  on the old number. Same class as, and the direct sibling of, the
  `parallax_height_scale` / `parallax_max_passes` case that `static_meshes.rs`
  **already** solves correctly in the same function — using
  `DEFAULT_PARALLAX_HEIGHT_SCALE` / `DEFAULT_PARALLAX_MAX_PASSES` by name, with
  a comment (#3073) saying it "shares the same named default rather than its
  own independently-typed magic number". The doctrine was applied to one pair
  of defaults and not to the sibling pair added later.
- **Related**: #3073.
- **Suggested Fix**: Import the two constants at all three sites, exactly as
  the parallax pair already is.

---

### REN-2026-09-05-D6-03: two stale NIFAL facts in the shared audit skill files

- **Severity**: LOW
- **Dimension**: NIFAL Material
- **Location**: `.claude/commands/_audit-common.md` (the "NIFAL Translate"
  project-layout row); `.claude/commands/audit-renderer/SKILL.md`
  (Dimension 6 checklist)
- **Status**: NEW
- **Description**: Two facts an auditor is meant to check the code against are
  themselves wrong.
  1. `_audit-common.md` states `MaterialTextureSet<T>` "replaces per-game
     texture slot numbers with **18 named** source-agnostic roles +
     `decals: [T; 4]`". The live struct in `crates/nif/src/import/types.rs`
     declares **22** named roles plus the four decals — 26 slots.
  2. `audit-renderer/SKILL.md` Dimension 6 states `translate_material` "has
     **exactly two callers** — `byroredux/src/scene/nif_loader.rs` (loose NIF)
     and *byroredux/src/cell_loader/spawn.rs* (REFR placement). A third
     `Material {…}` literal downstream is a translation leak." There are three
     production callers today, and the second path moved into a subdirectory.
- **Evidence**: (1) field-count scan of the struct body, and the passing
  `documented_texture_role_list_matches_the_struct` test in
  `byroredux/src/material_translate.rs`, which pins the "N named roles" prose
  for `docs/engine/nifal.md` and `.claude/commands/audit-nifal/SKILL.md` —
  those two say 22 — but does **not** cover `_audit-common.md`. (2) production
  callers are `byroredux/src/scene/nif_loader.rs`,
  `byroredux/src/cell_loader/spawn/mesh_instance.rs` and
  `byroredux/src/cell_loader/placement_lod.rs` (the exterior placement-LOD
  spawner, added under #2444); the fourth call site, in
  `byroredux/src/cornell.rs`, sits inside that file's `#[cfg(test)]` module.
- **Impact**: (1) `_audit-common.md` is the file every audit skill reads
  first, so the wrong count propagates into every audit that walks the role
  set — and it is precisely the count #3465 was raised to stop drifting; the
  guard that fix installed simply does not reach this file. (2) An auditor
  applying the "exactly two callers" rule literally reports
  `placement_lod.rs` as a translation leak, which is a false positive — the
  real guard is `every_exterior_spawner_inserts_a_boundary_material`, which
  scans `cell_loader/*.rs` and requires every `MeshHandle`-spawning file to
  route through a boundary function, and it passes. Both are instances of the
  standing "verify the audit premise against current code" rule failing at the
  premise's own source.
- **Related**: #3465, #2444, #1114 (the path/symbol-reference convention).
- **Suggested Fix**: Correct both statements; extend
  `documented_texture_role_list_matches_the_struct` to cover
  `.claude/commands/_audit-common.md` alongside the two files it already pins;
  and reword the Dimension 6 caller bullet to point at the boundary *test*
  rather than a hand-maintained caller count, which is the structural fix
  #3733 already applied to the sibling spawner list.

---

## Consumption-Path Assessment

**What is well-guarded (checked, no finding).** These are recorded so a later
run does not re-derive them:

- **Role vocabulary is source-agnostic end to end.** 22 named roles + 4 decals
  in `MaterialTextureSet`; `roles()`, `map_ref` and `zip_map_ref` all enumerate
  the same set; the pipeline-stage change (`Option<FixedString>` → `Option<String>`
  → bindless `u32`) goes through `map_ref`, so a new role cannot be silently
  dropped in transit.
- **Role → bindless index** (`map_secondary_texture_handles`,
  `byroredux/src/asset_provider/texture.rs`) is an exhaustive struct literal —
  adding a role to `MaterialTextureSet` fails to compile until it is resolved
  here — with a per-role sRGB/linear decision and a cubemap arm for
  `environment`. The same helper serves the loose-NIF and cell spawn paths, so
  the old slot-divergence cannot return.
- **REFR overlay does not drop unhandled roles.** `mesh_instance.rs` seeds its
  working set from `mesh.material.textures.map_ref(...)` before applying any
  `resolve_effective` arm, so `dark`, `reflectance`, `emittance_gradient`, the
  two glass roles and the decals — none of which have a TXST wire-slot analog —
  ride through from the mesh rather than resetting to `None`.
- **Slot polymorphism is resolved once, at import, per game.**
  `slot_to_role` / `slot_to_colocated_role`
  (`crates/nif/src/import/material/slot_role.rs`) key on
  `TextureSlotLayout` × shader type × feature flags, and the REFR overlay path
  consults the *same* table with the *same* context (#2695, #3732) rather than
  a second flat table. Colocation (Skyrim slot 2 = `Tint` **and**
  `LightingMask`) is expressed explicitly instead of being lost.
- **No per-game branch reaches the renderer.** Grep for game names across
  `byroredux/src/render/{static_meshes,particles,mod}.rs` and
  `crates/renderer/src/vulkan/material.rs` returns comments only; every live
  branch keys on the canonical `material_kind` or on a `MAT_FLAG_*` bit
  generated from `crates/renderer/src/shader_constants_data.rs`.
- **Dedup key integrity.** `hash_gpu_material_fields` and
  `DrawCommand::material_hash` walk 108 lanes in the same order;
  `material_hash_matches_gpu_material_field_hash` pins them against a fixture
  whose `supplemental_texture_indices` is `[31, 32, … 46]` — distinct per slot,
  so the pin genuinely catches a slot mis-order between the two walks.
- **Cap and telemetry.** `intern_by_hash` routes over-cap materials to id 0 and
  counts them; `upload_materials` hard-asserts the cap in release and
  content-hash dirty-gates the copy; `ctx.scratch` prints unique/interned/ratio
  and appends the overflow count. All three checklist items hold.
- **Flag-gating of the new mask roles.** `lightingMaskMapIndex` and
  `backLightingMapIndex` are sampled unconditionally on non-zero index but
  *applied* only behind `MAT_FLAG_SOFT_LIGHTING` / `MAT_FLAG_RIM_LIGHTING` /
  `MAT_FLAG_BACK_LIGHTING` in `crates/renderer/shaders/include/lighting.glsl`,
  with a white (`vec3(1.0)`) identity default — so an authored mask on a
  material whose feature flag is clear cannot darken it.
- **Direct-sun and clustered light paths agree.** Both route through
  `shadowableLightRadiance`, which takes `lightingMask` and `backLightingMap`
  as parameters; there is no second, mask-free direct arm.

**Documented deferrals confirmed intact (do not re-report as gaps).**
A field-by-field scan of the 108 GLSL `GpuMaterial` lanes against every
`.vert`/`.frag`/`.comp` and `crates/renderer/shaders/include/*.glsl` finds
**sixteen** that no shader reads off the material record. Fifteen of them are
documented:

- `lightingMapIndex`, `flowMapIndex`, `wrinkleMapIndex` — deferral recorded at
  both the Rust field and the GLSL mirror (#2712). `flowMapIndex` also occurs
  in `crates/renderer/shaders/water.frag`, but that is water's own
  push-constant lane, not `materials[...]`, so the note is accurate.
- `shaderColorR/G/B`, `shaderFloat` — same shape (#2221).
- The EyeEnvmap payload (`eyeLeftCenterX/Y/Z`, `eyeCubemapScale`,
  `eyeRightCenterX/Y/Z`) and `multiLayerRefractionScale` — parked in prose in
  `crates/renderer/shaders/triangle.frag`'s "Variant stubs" block rather than
  at the field itself, which is a weaker but real record.

The sixteenth is `textureIndex`, reported above as REN-2026-09-05-D7-02.

---

## Prioritized Fix Order

1. **REN-2026-09-05-D7-01** (MEDIUM) — the only finding that changes rendered
   pixels. Decide wire-vs-defer before touching anything else here, because the
   decal half also determines the secondary-ray coverage contract.
2. **REN-2026-09-05-D6-01** and **REN-2026-09-05-D6-03** (LOW, doc-rot) — false
   premises that will mislead the *next* audit of this exact seam. Cheapest
   fixes, highest leverage on future audit accuracy. Fold the
   `_audit-common.md` half of D6-03 into the existing
   `documented_texture_role_list_matches_the_struct` guard so it cannot recur.
3. **REN-2026-09-05-D7-03** and **REN-2026-09-05-D7-04** (LOW, test-gap) —
   restore the two guards to full coverage of the grown role set. D7-04 is the
   one that would catch a genuine per-game mis-shade.
4. **REN-2026-09-05-D7-02** (LOW) — decide and record; measure any dedup-key
   change against `ctx.scratch` rather than reasoning about it.
5. **REN-2026-09-05-D6-02** (LOW) — mechanical constant substitution at three
   sites.

## Needs-RenderDoc

None. Neither dimension in this narrowed run touches render-pass, pipeline,
barrier or descriptor-lifetime state, so no finding here has a failure mode
invisible to `cargo test`. REN-2026-09-05-D7-01 is verifiable by a frame
capture or an A/B screenshot on a `dark`-mapped Oblivion interior and a
tint-family Skyrim head, but its *premise* is settled by source inspection and
needs no capture to accept.

---

*Report generated by `/audit-renderer --focus 6,7` under the
`texture-roles-deep` preset. Dimensions 1–5 and 8–23 were not run.*
