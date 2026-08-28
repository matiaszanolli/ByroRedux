# NIFAL Audit — 2026-08-27

Scope: all 9 dimensions, all games (Skyrim SE measured against real archives).
Solo (non-fanned-out) run, executed by reading / grepping / tracing directly
against the live tree, plus one purpose-built corpus census over
`Skyrim - Meshes0.bsa` + `Skyrim - Meshes1.bsa` (22,047 NIFs, 73,125
`BSLightingShaderProperty` blocks). Baseline for the delta:
`docs/audits/AUDIT_NIFAL_2026-08-24.md` (0 findings), diffed through
`147daae7..HEAD`.

## Executive Summary

**5 findings: 1 HIGH, 1 MEDIUM, 3 LOW.**

The three days since the last sweep were the busiest NIFAL window in a month:
the canonical `Material` grew the BGEM v21+ glass-optics tail
(`glass_fresnel_color` / `glass_refraction_scale` / `glass_blur_scale` /
`glass_blur_scale_factor`), the Oblivion sampler/blend triple
(`texture_clamp_mode` / `src_blend_mode` / `dst_blend_mode`, #2571), the
Skyrim soft/rim/back-light feature gates, and four new
`MaterialTextureSet` roles (`lighting_mask`, `back_lighting`,
`glass_roughness_scratch`, `glass_dirt_overlay`, taking the vocabulary from
18 to 22 named roles). A second marker-component boundary
(`attach_blend_and_facing_markers`, #2490) and a `Material::sanitize_finite`
repair pass (#2687 / #3373) also landed, and `sanitized_clip_frequency`
(#3258) closed the Dimension-7 cross-reference the 08-24 report carried
forward.

Almost all of that landed clean: `translate_material` is still the sole
`ImportedMaterial → Material` producer, the 22+4 texture roles are
acquire/release-symmetric through `values()` / `secondary_values()`, the
`GpuMaterial` byte contract is pinned at 432 B with every new field mirrored
in `include/bindings.glsl`, and `triangle.frag` + `include/*.glsl` still carry
**zero** `if game ==` branches.

The one HIGH is in the newest of that work. Skyrim's `SLSF2_Soft_Lighting`
feature gate now crosses the boundary, but for the **tint family**
(`FaceTint` / `SkinTint` / `HairTint`) the slot-2 texture the gate is supposed
to be masked by is routed to `TextureRole::Tint` and never reaches
`lighting_mask`. Measured on the vanilla archives that is **4,054 of 8,058
(50.3%)** of every soft-lighting property in the game — every FaceGen head and
every skin-tinted body/armour piece — and for all of them `triangle.frag`
substitutes an unauthored `vec3(1.0)`, so the wrap-lighting lobe runs at full
strength over the whole surface.

The MEDIUM is the shader-side twin of the still-open #3073: `triangle.frag`
hard-codes `Material::default()`'s glass-optics values (`/ 0.4`, `/ 0.05`) as
its normalization pivots, when this codebase already has the exact mechanism
for that (`shader_constants.rs`'s `DEFAULT_WATER_WAVE_AMPLITUDE` precedent,
with a test pinning the shader's use of the macro).

Two items found during this sweep were verified as **already filed by
concurrent audits** and are cross-referenced below rather than re-filed:
`AnimationClip.duration` / `.weight` crossing `convert_nif_clip` unvalidated
(SAFE-2026-08-27b-01), and `terrain_lod_btr.rs` spawning drawn entities with
no canonical `Material` (FNV-D2-03). The renderer audit's FO4 `Rimlight Power`
FLT_MAX sentinel and rim-lobe clamp-floor findings are likewise not re-filed.

## Per-Category Tier Matrix

| Category | single-boundary | no-fabrication | no-leak | no-render-time-fallback | Notes |
|---|---|---|---|---|---|
| Material (Dim 1) | PASS — `translate_material` (3 production callers: `scene/nif_loader.rs:915`, `cell_loader/spawn/mesh_instance.rs:632`, `cell_loader/placement_lod.rs:527`; `cornell.rs:1994` is the synthetic harness) | PASS | PASS | PASS | `#3073` (parallax scalars bypass the boundary) still OPEN, not re-reported |
| Material markers (Dim 1) | **PASS — NEW** `attach_blend_and_facing_markers` (#2490), pinned by `both_spawn_sites_derive_markers_through_this_boundary` | PASS | PASS | PASS | new boundary this window |
| Mesh water (Dim 1) | PASS — `attach_mesh_water` | PASS | PASS | — | unchanged |
| Geometry/Transform (Dim 2) | PASS | PASS | PASS | PASS | `canonical_mesh_path` (#2361/#3391) is parse-side, byte-neutral |
| Skinning (Dim 3) | PASS | — | documented gap (#2440, unchanged) | — | #3355/#3360 index-space fixes are parse-side |
| Lights (Dim 3) | PASS — `translate_light` + `LightSource::from_legacy_world_units`; `spawn.rs:920` `world_direction` rotation intact | PASS | PASS | — | 08-23 fix re-verified |
| Nodes (Dim 4) | N/A by design | — | PASS — 7 parked fields, **0** canonical consumers (re-grepped) | — | unchanged |
| Particles (Dim 5) | PASS — `apply_emitter_overlays`, both load paths | PASS | PASS | PASS | `#2610` still open, not re-reported |
| Collision (Dim 6) | PASS | PASS | PASS | — | **16/16** `bhk*Shape` resolve arms re-counted; `segment_shape` (#3317) keeps origin-centred/Y-aligned segments as bare primitives |
| Animation (Dim 7) | PASS — `convert_nif_clip` | **PASS — FIXED** (`sanitized_clip_frequency`, #3258) | partial — `duration`/`weight` still raw (owned by SAFE-2026-08-27b-01) | — | 08-24 cross-reference now closed for `frequency` |
| Shader flags / texture roles (Dim 8) | PASS | **FAIL** — NIFAL-2026-08-27-01 | **FAIL** — NIFAL-2026-08-27-01 | PASS — zero `if game ==` in `triangle.frag` + `include/*.glsl` (re-verified) | `values()` ↔ struct parity re-verified at 22 named + 4 decals = 26 |
| GPU contract (Dim 8/9) | — | **FAIL (MEDIUM)** — NIFAL-2026-08-27-02 | — | — | `GpuMaterial` still 432 B, offsets 364-428 pinned, GLSL mirrored |
| Completeness signal (Dim 9) | — | — | — | PASS | harness coverage hole filed as NIFAL-2026-08-27-04 |

## Findings

### HIGH

#### NIFAL-2026-08-27-01: Skyrim's `SLSF2_Soft_Lighting` gate crosses the boundary without its slot-2 mask on the tint family — 50.3% of every soft-lighting property in the vanilla game, and the shader substitutes an unauthored `vec3(1.0)`

- **Severity**: HIGH
- **Dimension**: Shader-flags / texture sets (Dim 8)
- **Tier Violated**: `no-leak` (the authored slot-2 texture reaches only one of the two roles the wire format multiplexes onto it) + `no-fabrication` (the consumer invents the missing half)
- **Game Affected**: Skyrim SE / LE (the `TextureSlotLayout::Skyrim` arm is the only one that sets these gates)
- **Location**: `crates/nif/src/import/material/slot_role.rs:236-248` (the slot-2 arm), `crates/nif/src/import/material/dedicated_shader.rs:142-157` (the gate extraction), `byroredux/src/material_translate.rs:523-525` + `byroredux/src/cell_loader.rs:267-275` (the gate crossing), `crates/renderer/shaders/triangle.frag:2659-2663` (the substituted mask), `crates/renderer/shaders/include/lighting.glsl:92-108` (the consumer)
- **Status**: NEW — the consumer and the gate both landed in this audit window
- **Description**:

  `apply_bs_lighting_shader` reads Skyrim's SLSF2 bits 25/26/27 into
  `MaterialInfo.{soft,rim,back}_lighting` **unconditionally for the Skyrim slot
  layout**, and `pack_imported_material_flags` turns them into
  `MAT_FLAG_SOFT_LIGHTING` / `_RIM_` / `_BACK_` on the canonical `Material`.
  Separately, `slot_to_role` decides where the property's slot-2 texture lands,
  and its very first test is the tint family:

  ```rust
  // crates/nif/src/import/material/slot_role.rs:236-248
  (TextureSlotLayout::Skyrim | TextureSlotLayout::Starfield, 2) => {
      if tint_family {
          Some(TextureRole::Tint)
      } else if context.glow_map {
          Some(TextureRole::Emissive)
      } else if context.soft_lighting || context.rim_lighting {
          Some(TextureRole::LightingMask)
      } else {
          None
      }
  }
  ```

  So on a `FaceTint` (4) / `SkinTint` (5) / `HairTint` (6) property the authored
  `*_sk.dds` becomes `Tint` and `MaterialTextureSet::lighting_mask` stays
  `None` — while `MAT_FLAG_SOFT_LIGHTING` crosses regardless. The renderer then
  fills the hole itself:

  ```glsl
  // crates/renderer/shaders/triangle.frag:2659-2663
  vec3 lightingMask = vec3(1.0);
  if (mat.lightingMaskMapIndex != 0u) {
      lightingMask = texture(
          textures[nonuniformEXT(mat.lightingMaskMapIndex)], sampleUV).rgb;
  }
  ```

  and `bethesdaDiffuseLightFactor` mixes at that weight:

  ```glsl
  // crates/renderer/shaders/include/lighting.glsl:96-107
  if ((mat.materialFlags & MAT_FLAG_SOFT_LIGHTING) == 0u) return vec3(front);
  float width = mat.subsurfaceRolloff > 0.0
      ? mat.subsurfaceRolloff : mat.lightingEffect1;
  width = clamp(width, 0.0, 4.0);
  float wrapped = max((rawNdotL + width) / (1.0 + width), 0.0);
  return mix(vec3(front), vec3(wrapped), clamp(lightingMask, 0.0, 1.0));
  ```

  There is no `material_kind` gate on that call — it runs from the main lit
  loop (`triangle.frag:2856`) and from every `shadowableLightRadiance` call
  site — so the whole tint-family surface takes the wrapped lobe at full
  weight.
- **Evidence**: census over `Skyrim - Meshes0.bsa` + `Skyrim - Meshes1.bsa`
  (22,047 NIFs, `100 <= bsver < 130`), classifying each
  `BSLightingShaderProperty` that sets SLSF2 bit 25 by whether its slot-2
  texture actually reaches `TextureRole::LightingMask`:

  ```
  BSLightingShaderProperty props = 73125
  SLSF2_Soft_Lighting            = 8058
    routed to LightingMask       = 3975
    tint-family (slot 2 -> Tint) = 4054      <- gate crosses, mask does not
    slot 2 empty                 =   24
    Glow_Map wins slot 2         =    5
    UNMASKED with lighting_effect_1 > 0 = 4083   (all of them)
  SLSF2_Rim_Lighting  = 256
  SLSF2_Back_Lighting = 2063   (36 with an empty slot 7)
  ```

  Every one of the 4,054 tint-family cases has a **non-empty** slot 2 (the
  bucket is `else if tint_family` after the empty-slot test) and
  `lighting_effect_1 == 0.4`, so `width = 0.4` and the substituted mask is
  fully load-bearing: `wrapped = (N·L + 0.4)/1.4` replaces `max(N·L, 0)` across
  the entire surface. Representative paths, transcribed from the census:
  `meshes\actors\character\facegendata\facegeom\skyrim.esm\0006765a.nif`
  (`ty=4`, slot 2 `Actors\Character\Male\MaleHead_sk.dds`),
  `meshes\armor\hide\f\cuirassheavychieftain_1.nif` (`ty=5`, slot 2
  `textures\actors\character\female\FemaleBody_1_sk.dds`),
  `meshes\clothes\archmage\m\archmagerobesm_1.nif` (`ty=5`).
  `slot_role.rs`'s own #2694 comment records that **3158/3158** vanilla
  FaceTint properties populate slot 2, so the population is total, not
  incidental.

  For contrast, the FO4/BGSM lane genuinely has no mask to lose —
  `forward_bgsm_rim_subsurface` (`byroredux/src/asset_provider/material.rs:77-97`)
  sets `soft_lighting` from `bgsm.subsurface_lighting` and BGSM authors no
  companion texture — so the unit default is defensible *there* and only there.
- **Impact**: Visual, no crash, but the blast radius is every NPC face and
  every skin-tinted body/armour surface in Skyrim, plus the ~3,975 correctly
  masked non-tint materials are shaded by a different rule than their
  tint-family neighbours. It is a **behaviour change introduced in this audit
  window** (before the soft/rim/back work the lobe did not exist at all), and
  nothing in the test suite can see it: `slot_to_role`'s own new test
  (`skyrim_feature_flags_route_soft_rim_and_back_lighting_maps`) builds its
  context with `skyrim(0, …)`, i.e. shader type 0, which is exactly the arm
  that *does* work.

  The remediation is a ground-truth question this audit deliberately does not
  answer by guessing (`feedback_no_guessing`). nif.xml documents slot 2 as
  `Glow(SLSF2_Glow_Map)/Skin/Hair/Rim light(SLSF2_Rim_Lighting)`
  (`/mnt/data/src/reference/nifxml/nif.xml:6313`) — it attributes the slot to
  Skin/Hair **and** to rim light, and does not mention soft lighting at all.
  So one texture legitimately serves two simultaneous roles on the tint family,
  and the canonical `MaterialTextureSet` model (one slot → at most one role)
  cannot currently express that. What is not defensible either way is the
  present state: the gate crosses the boundary while its mask does not, and the
  shader silently picks the *maximally active* substitute.
- **Related**: `#2694` (the fix that gave the tint family slot 2), `#3068` (the
  fix that stopped slot 2 becoming self-illumination without a flag),
  REN-2026-08-27 rim-lobe clamp floor (the sibling defect in the same three new
  lobes — do not merge the two, that one is the exponent, this one is the mask)
- **Suggested Fix**: Decide the coupling explicitly at the boundary rather than
  at the shader default. Either (a) let slot 2 fill **both** `tint` and
  `lighting_mask` when the tint family also sets `Soft_Lighting`/`Rim_Lighting`
  — the wire format multiplexes it, so `slot_to_role` returning a single role
  is the model gap, not the data — or (b) clear
  `MaterialInfo.{soft,rim}_lighting` when no slot-2 texture reached
  `LightingMask` on a Skyrim property, so the gate and its mask cross together
  and the shader's `vec3(1.0)` stays reachable only for the BGSM lane that
  genuinely has no mask. Whichever is chosen, extend
  `skyrim_feature_flags_route_soft_rim_and_back_lighting_maps` to cover
  `shader_type` 4/5/6 — the arm that carries 50% of the content.

### MEDIUM

#### NIFAL-2026-08-27-02: `triangle.frag` hard-codes `Material::default()`'s glass-optics values as its normalization pivots, when `shader_constants.rs` exists precisely to stop that drift

- **Severity**: MEDIUM
- **Dimension**: Material (Dim 1) / GPU contract (Dim 9)
- **Tier Violated**: `no-fabrication` (a canonical default is restated, unguarded, downstream of the boundary)
- **Game Affected**: all FO76 / Starfield / FO4 BGEM glass; and every engine-classified `MATERIAL_KIND_GLASS` surface on every game, since the pivot is applied unconditionally inside the `isGlass` branch
- **Location**: `crates/renderer/shaders/triangle.frag:1498-1500` and `:1864`; the canonical values they restate are `crates/core/src/ecs/components/material.rs:536-539`; the mechanism they should use is `crates/renderer/src/shader_constants.rs:215-216`
- **Status**: NEW
- **Description**:

  The BGEM v21+ glass tail reaches the shader correctly (all four scalars are
  in the 432-byte `GpuMaterial` at offsets 364-388, mirrored in
  `include/bindings.glsl:225-228`, and both maps are sampled). But the shader
  normalizes two of them by literals that are copies of the canonical
  `Material::default()` values:

  ```glsl
  // crates/renderer/shaders/triangle.frag:1498-1500
  float blurFactor = max(
      mat.glassBlurScale * mat.glassBlurScaleFactor, 0.0) / 0.4;

  // crates/renderer/shaders/triangle.frag:1864
  float deviationScale = clamp(mat.glassRefractionScale / 0.05, 0.0, 4.0);
  ```

  `0.4` is `Material::default().glass_blur_scale` and `0.05` is
  `Material::default().glass_refraction_scale`
  (`crates/core/src/ecs/components/material.rs:537-538`). They are the
  *neutral* pivots — the shader's own comment says so ("The format defaults
  … normalize to the established glass result", `:1490-1494`) — so changing
  either default in Rust silently shifts what "neutral" means on the GPU, with
  no compile error, no layout assertion (the offsets are pinned, the *values*
  are not), and no test.

  This codebase already solved this exact problem once. `shader_constants.rs`
  emits the canonical water defaults as GLSL macros:

  ```rust
  // crates/renderer/src/shader_constants.rs:215-216
  ("DEFAULT_WATER_WAVE_AMPLITUDE", format!("#define DEFAULT_WATER_WAVE_AMPLITUDE {DEFAULT_WATER_WAVE_AMPLITUDE:?}")),
  ("DEFAULT_WATER_WAVE_FREQUENCY", format!("#define DEFAULT_WATER_WAVE_FREQUENCY {DEFAULT_WATER_WAVE_FREQUENCY:?}")),
  ```

  and the accompanying test asserts both that the macro value tracks
  `WaterMaterial::default()` and that the shader divides by the macro rather
  than a literal (`shader_constants.rs:693-705`) — the identical
  divide-by-the-neutral-default shape.
- **Evidence**: `include/shader_constants.glsl:174-175` currently defines only
  the two water macros; grepping the generated header for any glass or material
  default returns nothing, while `triangle.frag` contains the two bare literals
  quoted above.
- **Impact**: No wrong value today — the four literals agree with `Material`.
  What is broken is the guarantee, and it is the same guarantee `#3073`
  (parallax defaults duplicated at six sites, still OPEN) is filed against,
  one tier further downstream: this copy lives in GLSL, where neither
  `cargo test` nor `cargo check` can see it at all. A future adjustment to the
  BGEM neutral (or a correction to the parsed format default) changes the
  authored-to-rendered mapping for every glass surface in every game while
  every test stays green.
- **Related**: `#3073` (the CPU-side twin, OPEN), `#2589` (the precedent that
  fixed `grayscale_to_palette_scale` / `fresnel_power` neutral defaults rather
  than leaving `0.0` in the struct literals), `#2514`
- **Suggested Fix**: Export `DEFAULT_GLASS_BLUR_SCALE` and
  `DEFAULT_GLASS_REFRACTION_SCALE` from `shader_constants.rs`, sourced from
  `Material::default()`, replace both literals with the macros, and mirror the
  water test's two assertions (macro tracks the Rust default; shader divides by
  the macro).

### LOW

#### NIFAL-2026-08-27-03: `Material::{soft_lighting, rim_lighting, back_lighting}` are write-only — the same fact already rides `effect_shader_flags`, and only the packed word has a consumer

- **Severity**: LOW
- **Dimension**: Material (Dim 1)
- **Tier Violated**: `no-leak` (two canonical representations of one fact, one of them unread)
- **Game Affected**: Skyrim (NIF gates), FO4+ (BGSM gates)
- **Location**: `crates/core/src/ecs/components/material.rs:291-297` (the fields), `byroredux/src/material_translate.rs:523-525` (the only writer), `byroredux/src/cell_loader.rs:267-275` (the packer, which reads `ImportedMaterial`, not these)
- **Status**: NEW
- **Description**: `translate_material` copies the three authored gates onto
  the canonical `Material` *and* — in the same call — packs
  `MAT_FLAG_SOFT_LIGHTING` / `_RIM_` / `_BACK_` into `effect_shader_flags` via
  `pack_imported_material_flags(source)`, which takes `&ImportedMaterial` and
  therefore derives the bits from the raw tier, not from the canonical bools it
  just wrote. The shader reads only the packed word
  (`include/lighting.glsl:96`, `:111`, `:119`). A full-tree grep for
  `.soft_lighting` / `.rim_lighting` / `.back_lighting` outside the BGSM crate,
  the NIF importer and tests finds no reader of the `Material` fields at all.
- **Evidence**: the three writer lines in `material_translate.rs:523-525` and
  the three `if material.<x>_lighting` packer lines in `cell_loader.rs:267-275`
  — the latter's `material` binding is the `&ImportedMaterial` parameter, one
  tier below. `Material` is a save/restore unit
  (`crates/save/src/driver.rs::restore_world`), so both representations
  round-trip independently and nothing reconciles them.
- **Impact**: None today; both are written from the same source in the same
  call. It is a latent divergence surface (any future `mat.set`-style editor,
  or a restored save, can move one without the other) and a small maintenance
  cost — a reader who finds the canonical bool reasonably assumes it is what
  the renderer consults. Note this is *not* the `mesh_instance.rs:193-195`
  `TextureSlotContext` re-read of the raw tier: that runs in
  `resolve_mesh_paths`, **before** `translate_material`, so it structurally
  cannot read the canonical component.
- **Related**: `#2571` / OBL-D5-01 (the same "spawn sites should read the
  canonical component" argument, applied three lines away for
  `texture_clamp_mode` / `src_blend_mode` / `dst_blend_mode`)
- **Suggested Fix**: Either derive the three flag bits from the canonical bools
  after the literal is built (one representation feeds the other), or drop the
  bools and keep `effect_shader_flags` as the single canonical carrier — the
  same call this audit's HIGH finding also touches, so the two are worth fixing
  together.

#### NIFAL-2026-08-27-04: the canonical-completeness harness's "reverting any single `source.X` line fails an assertion" contract is false for four fields, two of which gate the NIFAL↔WATAL seam

- **Severity**: LOW (test-coverage gap; the code is correct today)
- **Dimension**: Completeness signal (Dim 9)
- **Tier Violated**: `parked-not-leak` verification gap
- **Game Affected**: all
- **Location**: `byroredux/src/material_translate.rs:1904-2010` (`translate_material_copies_every_canonical_field`), against the copies at `:441-442`, `:534`, `:545`, `:504-508`
- **Status**: NEW — the residual of `#2214` / NIFAL-D9-02
- **Description**: The harness's doc-comment states *"Deliberately reverting any
  single `source.X` → `material.X` line in `translate_material` fails exactly
  the corresponding assertion below — this is the 'fails on a deliberately
  reintroduced boundary drop' contract #2214 asked for."* Four copies have no
  corresponding assertion: `water_shader_flags` (`:441`), `is_water_shader`
  (`:442`), `ior` ← `material_optical_scalar(material_kind,
  refraction_strength)` (`:545`), and `effect_shader_flags` (`:504-508`).
  `grayscale_to_palette_scale` (`:534`) is uncovered by the harness but *is*
  covered by a dedicated sibling test
  (`translate_material_copies_grayscale_to_palette_scale`, `:1497-1507`), so it
  is not part of this gap.

  `is_water_shader` matters most: it is the sole gate both spawn sites read to
  decide whether to call `attach_mesh_water` (`scene/nif_loader.rs:925`,
  `cell_loader/spawn/mesh_instance.rs:825`), i.e. whether a dedicated
  `WaterShaderProperty` mesh crosses the NIFAL↔WATAL seam at all. Silently
  changing that copy to `false` removes every mesh-authored water plane in
  every game and the whole suite stays green.
- **Evidence**: the `kitchen_sink_source()` fixture sets neither
  `water_shader_flags` nor `is_water_shader` (both fall to
  `..ImportedMaterial::default()`), and the assertion block at `:1907-1949`
  contains no `material.water_shader_flags` / `material.is_water_shader` /
  `material.ior` / `material.effect_shader_flags` line. The `Material` literal
  in `translate_material` has no `..Default::default()` tail, so deleting a
  line is a compile error — the reachable regression is a line *changed* to a
  constant, which is exactly what the harness claims to catch.
- **Impact**: None today. The harness is the designated whole-boundary guard
  (`#2214` was filed because `crates/nif`'s raw-tier harness physically cannot
  reach `translate_material`), so a false completeness claim on it is the same
  defect shape SAFE-2026-08-27b-03 raises for `sanitize_finite`'s hand-typed
  field list.
- **Related**: `#2214` / NIFAL-D9-02, SAFE-2026-08-27b-03 (the same
  hand-transcribed-list class), `docs/engine/watal.md`
- **Suggested Fix**: Add the four assertions with distinctive fixture values
  (`water_shader_flags: 0x5A`, `is_water_shader: true` — noting the fixture
  must then not exercise the water path, or split into a second fixture;
  `refraction_strength` + `material_kind = MATERIAL_KIND_FIRE_REFRACTION` for
  `ior`; a non-zero `effect_shader` for the packed word). Better still, follow
  the `shader_constants.rs` / `skinned_blas_refit.rs` precedent already used
  elsewhere in this repo and add an `include_str!("material_translate.rs")`
  scan asserting every `<field>: source.<field>` line in the `Material` literal
  has a matching `assert` in the harness.

#### NIFAL-2026-08-27-05: doc rot — the texture vocabulary is 22 named roles, not 18, and the two-phase boundary now has three `translate_material` callers, not "both spawn sites"

- **Severity**: LOW
- **Dimension**: Shader-flags / texture sets (Dim 8) + Material (Dim 1)
- **Tier Violated**: — (documentation)
- **Game Affected**: all
- **Location**: `docs/engine/nifal.md:489`, `.claude/commands/audit-nifal/SKILL.md:240`, `docs/engine/nifal.md:587`, `byroredux/src/material_translate.rs:43-46`, and the stale spawn-site line numbers in `byroredux/src/render/static_meshes.rs:415-418`
- **Status**: NEW
- **Description**: Three separate staleness items, all introduced by this
  window's work:

  1. `nifal.md:489` — *"Its 18 named roles plus four ordered decal layers"* —
     and the identical claim with an explicit 18-item list in
     `SKILL.md:240`. `MaterialTextureSet` now has **22** named roles
     (`crates/nif/src/import/types.rs:309-343`): the list is missing
     `lighting_mask`, `back_lighting`, `glass_roughness_scratch` and
     `glass_dirt_overlay`. `values()` and its parity test agree at 22 + 4 = 26
     (`types.rs:381-407`, `canonical_iteration_covers_every_role_once`), so
     only the prose is wrong — but the SKILL text is the checklist an auditor
     diffs `values()` against, which is the one hand-written role walk the
     compiler does not protect.
  2. `nifal.md:587` and `material_translate.rs:43-46` both say the Phase-2
     resolvers *"run at **both** spawn sites"*. There are now three production
     `translate_material` callers; `cell_loader/placement_lod.rs:527` is the
     third and calls neither resolver. That is harmless today — it attaches no
     `MaterialTextureHandles`, so both resolvers would early-return — but
     "both" no longer identifies the set, and the reason the third is exempt is
     recorded nowhere.
  3. `static_meshes.rs:415-418` still cites *"both spawn sites
     (cell_loader/spawn.rs:841 and scene/nif_loader.rs:793)"* as the audit
     evidence for the deleted render-side glass heuristic. The cell-path
     `Material` construction moved to `cell_loader/spawn/mesh_instance.rs`
     under `#2057`; neither line number resolves.
- **Evidence**: `grep -c` on the struct gives 22 `pub <role>: T` fields plus
  `decals: [T; 4]`; `grep -n "translate_material(" byroredux/src` gives three
  production call sites; `sed -n '841p' byroredux/src/cell_loader/spawn.rs` is
  unrelated code.
- **Impact**: Documentation only, but item 1 degrades the very checklist the
  Dimension-8 role-walk audit depends on, and item 3 is the kind of stale
  citation `#1114`'s path-reference convention exists to prevent.
- **Related**: `#1114` (path/symbol reference convention), `#2330` (the
  two-phase boundary), `#2057` (the split that moved the spawn site)
- **Suggested Fix**: Update the role count and list in both files; reword the
  Phase-2 sentence to "every `translate_material` caller that attaches
  `MaterialTextureHandles`" and note why `placement_lod` does not; refresh the
  two line numbers in `static_meshes.rs` (or drop them for symbol names, per
  the convention).

## Cross-Audit Notes (found here, already filed elsewhere — not re-reported)

- **SAFE-2026-08-27b-01** (`docs/audits/AUDIT_SAFETY_2026-08-27b.md`, MEDIUM) —
  `AnimationClip.duration` and `.weight` cross `convert_nif_clip`
  (`byroredux/src/anim_convert.rs:506`, `:520`) unvalidated while their
  sibling `frequency` (`:519`) is now sanitised by `sanitized_clip_frequency`.
  This sweep independently re-derived the same three-consecutive-lines gap and
  the same producer (`crates/nif/src/anim/sequence.rs:20-23`,
  `duration = seq.stop_time - seq.start_time`), and confirms the downstream
  NaN-transparency: `fold_reverse_time`'s only guard is `duration <= 0.0`
  (`crates/core/src/animation/player.rs:67`), which is `false` for NaN, and
  `sample_blended_transform` skips on `ew < 0.001`
  (`crates/core/src/animation/stack.rs:363`), also `false` for NaN. It is
  architecturally a Dimension-7 `no-leak` gap at this boundary; recorded here
  so a future NIFAL sweep does not rediscover it, but owned by the safety
  report.
- **FNV-D2-03** (`docs/audits/AUDIT_FNV_2026-08-26.md:2517`, LOW) —
  `byroredux/src/cell_loader/terrain_lod_btr.rs:361-393` spawns a drawn entity
  with `MeshHandle` + `TextureHandle` + `MaterialTextureHandles` and **no**
  canonical `Material`, so it falls into `render/static_meshes.rs`'s
  no-`Material` literal arm. Re-verified as still open this sweep; note the
  arm it falls into grew twelve more hard-coded `Material::default()` values
  in this window (`static_meshes.rs:678-696`), and the `#2444` source-scan
  guard `every_exterior_spawner_inserts_a_boundary_material`
  (`material_translate.rs:1570-1605`) still lists only four spawners, not this
  one.
- **REN-2026-08-27** (renderer audit, concurrent) — the FO4 `Rimlight Power`
  `FLT_MAX` parser sentinel crossing the boundary verbatim
  (`crates/nif/src/blocks/shader.rs:1070-1089` →
  `byroredux/src/material_translate.rs:520`) and the rim-lobe `0.0 → 0.25`
  clamp floor (`crates/renderer/shaders/include/lighting.glsl:110-116`).
  Both are in the same three new lobes as NIFAL-2026-08-27-01 and should be
  fixed in one pass, but they are the *exponent* problem; this report's HIGH is
  the *mask* problem. Distinct root causes, distinct fixes.

## Documented-Limitation Ledger (re-verified this cycle, not re-reported)

- **`#3073`** — `parallax_height_scale` / `parallax_max_passes` still bypass
  `translate_material` entirely: raw `Option<f32>` on `ImportedMaterial`
  (`crates/nif/src/import/types.rs:530-531`) resolved by
  `.unwrap_or(0.04)` / `.unwrap_or(4.0)` at
  `byroredux/src/scene/nif_loader.rs:1066-1067` and
  `byroredux/src/cell_loader/spawn/mesh_instance.rs:857-858`, plus the
  per-draw third copy at `byroredux/src/render/static_meshes.rs:302-307`.
  `MaterialTextureHandles` still declares no shared constant
  (`byroredux/src/components.rs:300-303`). OPEN, unchanged — filed
  2026-08-16 as NIFAL-D1-2026-08-16-01.
- **`#2440`** — cell-placed skinned geometry still renders in bind pose
  (loose-NIF path is the only `SkinnedMesh::new_with_global` producer).
- **Node passthroughs** — `bs_value_node`, `bs_ordered_node`, `tree_bones`,
  `range_kind`, `lod_group`, `bs_lod_cutoffs`, `bs_sub_index`: re-grepped this
  sweep, all seven still have **zero** canonical ECS consumers (every hit
  outside `types.rs` / the parser / tests is a `<field>: None` construction).
- **`HkPackedNiTriStripsData.sub_parts`** (`#2550`, new this window) — the
  FO3+ per-sub-part Havok filter/material table is now decoded rather than
  skipped and is deliberately parked with a documented reason and unblocking
  consumer (`crates/nif/src/blocks/collision/shape_mesh.rs:122-130`). Zero
  import consumers, correctly. **Parked, not a leak** — record it here so the
  next sweep does not file it.
- **`BhkNPCollisionObject` / `BhkPCollisionObject`** — FO4+ packed-Havok blob
  and Skyrim+ phantoms; the authoring-aware `CollisionAuthoringSummary`
  fallback is intact. Unchanged.
- **`BhkPlaneShape`** — still the one deliberate `None` arm of the 16
  (`#1334`), documented at its arm.
- **`#2610`** — particle `DrawCommand.effect_shader_flags` still hardcoded `0`
  (`byroredux/src/render/particles.rs:272`).
- **`#3187`** — `RefrTextureOverlay::apply_slot_swap` flat slot table;
  unchanged. Note the *other* half of the overlay path (`resolve_mesh_paths`)
  correctly routes through `slot_to_role` with the mesh's own shader type
  (`cell_loader/spawn/mesh_instance.rs:187-265`), including the two new roles.
- **`#2327`** / SKY-D7-02 — SLSF1 `Refraction` without `Fire_Refraction` still
  has no canonical field or shader consumer; deliberate, documented.
- **`#2443`** / MAT-D3-01 — `grayscale_to_palette_scale` now both reaches
  `Material` and is consumed by the shader (`GpuMaterial` offset 420); the
  ledger entry from prior sweeps is superseded, keep it closed.
- **Starfield particle slice N/A** (`#2354`) — pinned by
  `starfield_corpus_has_no_particle_blocks`; unchanged.
- **`NiTextureEffect` / `NiLODNode`** — content-absent, forward-compat only;
  unchanged.

## Verification Method

- Read `docs/engine/nifal.md` in full and the previous report
  (`AUDIT_NIFAL_2026-08-24.md`) before touching code; confirmed each of its
  four verified fixes is still in place (light-direction rotation at
  `cell_loader/spawn.rs:920`, morph `original_index`, BGSM
  `external_specular`, per-game completeness floors).
- Diffed `147daae7..HEAD` (the commit that added the 08-24 report) restricted
  to every NIFAL-relevant path in `_audit-common.md`'s layout, then read the
  full diff of each of the 58 touched files that mattered.
- Traced each of the four new `MaterialTextureSet` roles end to end: parser →
  `slot_to_role` → `MaterialInfo` → `ImportedMaterial` → `translate_material` /
  `MaterialTextureHandles` → `map_secondary_texture_handles` →
  `supplemental_texture_indices` → `GpuMaterial` → `triangle.frag`, and back out
  through `unload.rs`'s `secondary_values()` release walk.
- Re-counted the hand-written role walks: 22 named + 4 decals in the struct,
  22 + `.chain(decals)` in `values()`, 26 in `map_ref`, all three consistent;
  `secondary_values()` is still `values().skip(1)` with `base_color` first.
- Re-counted `resolve_shape_inner`'s `downcast_ref::<Bhk*Shape>` arms: **16**,
  matching `dispatch_coverage_tests`' own expectation.
- Re-grepped all seven parked `ImportedNode`/`ImportedMesh` fields for
  canonical consumers: zero.
- Re-verified `triangle.frag` + every `include/*.glsl` header carry zero
  `if game ==` / `GAME_` / per-title branches.
- **Corpus census** (the HIGH's evidence): built a throwaway
  `crates/nif/examples` probe against `byroredux-bsa` + `parse_nif`, ran it over
  `Skyrim - Meshes0.bsa` + `Skyrim - Meshes1.bsa` (22,047 NIFs), bucketing every
  `SLSF2_Soft_Lighting` property by whether its slot-2 texture reaches
  `TextureRole::LightingMask`. Probe deleted after the run; the tree is clean.
- Consulted `/mnt/data/src/reference/nifxml/nif.xml:6309-6318` (BSShaderTextureSet
  slot semantics) and `:6605-6609` (`Lighting Effect 1/2`, `Rimlight Power`)
  rather than inferring the slot vocabulary, and confirmed there are **no**
  `BSShaderCRC32` options for soft/rim/back lighting, so the Skyrim-only gate in
  `dedicated_shader.rs` is correct and its Starfield slot-arm is inert by design.
- Ran `cargo check -p byroredux-nif -p byroredux-core -p byroredux` (clean) and
  the relevant suites: `cargo test -p byroredux-nif --lib import::material::`
  (212 passed), `cargo test -p byroredux-core --lib material` (37 passed),
  `cargo test -p byroredux --bin byroredux material_translate::` (44 passed).
- Deduplicated every candidate against `docs/audits/` and `.claude/issues/`
  before writing: two candidates (the `duration`/`weight` boundary gap and the
  `terrain_lod_btr` missing `Material`) were dropped to cross-references after
  finding them already filed, and the parallax-scalar bypass was dropped after
  finding it tracked as OPEN `#3073`.

Suggest: `/audit-publish docs/audits/AUDIT_NIFAL_2026-08-27.md`
(domain label `nifal`; add `shaders` for NIFAL-2026-08-27-01 and -02,
`game:skyrim` for -01, `doc-rot` for -05, `test-gap` for -04.)
