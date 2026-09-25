# #4834 — REN-D1-2026-09-24-02: the kind-102 arm of `shadow_mask_for_instance` ignores blend state — non-blended `NoLighting` geometry (foliage, doors, armor, building shells) becomes a non-occluder for every light

**Labels**: bug,renderer,medium,game:fnv,game:fo3
**Filed from**: docs/audits/AUDIT_RENDERER_2026-09-24.md (audited `main` @ `6c5555c70`)

- **Severity**: MEDIUM. Rendering-correctness (visual) with a measured population of ~10² meshes per game, not the 4,699 that made REN-D1-2026-09-21-01 HIGH. Magnitude on screen is unmeasured.
- **Dimension**: AS Correctness (instance mask)
- **Location**: `crates/renderer/src/vulkan/acceleration/predicates.rs` — `shadow_mask_for_instance` (EFFECT arm: `material_kind == MATERIAL_KIND_NO_LIGHTING || (alpha_blend && dst_blend == GAMEBRYO_DST_BLEND_ONE)`) and its twin `mask_divert_cause`.
- **Status**: NEW (side effect of the #4576 fix; the closed issue's census counted blended cards only).
- **Description**: #4576 routed kind 102 to the EFFECT layer unconditionally. The function's own comment states the principle that material kind is the authored "not solid" signal and blend state alone is not. But kind 102 is the FO3/FNV shader class `BSShaderNoLightingProperty` ("fullbright/unlit"), applied to solid assets as well as FX. The arm therefore also moves **non-blended** NoLighting meshes into `VISIBILITY_LAYER_EFFECT`, which `traceShadowTransmittanceDetailed` excludes via `visibilityMask & VISIBILITY_MASK_ALL_OPAQUE` for every light, including `FULL`-mask lights. The other non-fragment tracers never include the EFFECT bit either (volumetrics visibility, caustic visibility ray, ground-cover tracers).
- **Evidence** (read-only real-data census, every NIF in the vanilla `Fallout - Meshes.bsa`, `material_kind == 102 && !has_alpha`): FNV 14,881 NIFs parsed — 4,566 blended kind 102, **1,281 not blended** (597 thin cards, 684 not thin, 59 not thin with >= 40 tris); FO3 10,989 NIFs — 2,137 blended, **415 not blended** (116 thin, 299 not thin, 17 >= 40 tris). Oblivion has no kind 102.
  Solid-looking samples: FNV `trees\nv_jtown_trees\nv_euroaspen01–03.nif` leaf meshes (944–1,104 tris), `dungeons\nvlucky38\*elevator*.nif` door panels, `armor\teslaekarmor\teslaekarmor.nif` `UpperBody3`, `architecture\urban\oldtown\otbldgdestroy0*.nif` building shells, `landscape\rocks\cliffs\canyoncavesentrance02bnv.nif` sub-mesh `:3`; FO3 `dungeons\pipe\lpipedoor01.nif`, `architecture\pentagon\citadelcenterstand02.nif`.
- **Impact**: Those meshes cast no direct shadow for any light, do not block sun shafts in the froxel grid, and drop out of the volumetric combustion-path, caustic-visibility and ground-cover placement rays. Reflection and GI rays (mask `0xFF`) still see them, so the scene is lit inconsistently with its geometry. The Dim 10 auditor adds a pre-existing parity gap that widens with this population: GI and reflection hits shade kind-102 surfaces as lit opaque diffuse quads although they are fullbright in raster and non-occluding in shadows.
- **Related**: #4576 (closed), REN-D1-2026-09-21-01, #3305 (open), `docs/engine/physical-lighting-backbone.md`.
- **Suggested Fix**: Restrict the kind-102 arm to `alpha_blend` (the census class the fix targeted), or carry an explicit canonical non-occluder bit across the NIFAL boundary as the prior audit suggested. Add a non-blended kind-102 fixture to `shadow_mask_bucket_selection_is_pinned`. **Needs an FNV A/B (Jacobstown aspens, Lucky 38 elevators, `rt.masks`) before choosing.**

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders, other producers/consumers, other games)
- [ ] **TESTS**: A regression test pins this specific fix (and fails when the guarded code is deleted — mutation-check it)
