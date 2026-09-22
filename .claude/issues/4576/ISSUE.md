# REN-D1-2026-09-21-01: removing the blend→EFFECT divert makes FO3/FNV/Oblivion blended FX cards (mist, ground fog, light beams, glow shells) binary shadow blockers

**Labels**: high, renderer, vulkan, bug, game:fnv, game:fo3, game:oblivion

Filed via /audit-publish from docs/audits/AUDIT_RENDERER_2026-09-21.md.

**Severity**: HIGH (rendering correctness on the default path in three games; per-cell magnitude not yet captured live) · **Dimension**: AS Correctness (instance mask)
**Location**: `crates/renderer/src/vulkan/acceleration/predicates.rs` `shadow_mask_for_instance` (~:976-1028; the blend arm is gone and `_alpha_blend` is unused), called from `acceleration/tlas.rs` (~:649). Consumers: `shaders/include/shadow_transport.glsl` `traceShadowTransmittanceDetailed`, `shaders/include/ray_hit.glsl` `rayHitHasCoverage` (~:438), `shaders/include/shadow_common.glsl` `traceShadowBinary` (volumetrics_inject / caustic_splat), `shaders/volumetrics_inject.comp` `combustionPathBlocked` (`VISIBILITY_MASK_SOLID`, ~:1719-1737)
**Status**: NEW (introduced by `f97775ca8`, 2026-09-21)
**Verified against**: HEAD `f97775ca8`

## Description

Until `f97775ca8`, every non-actor alpha-blended instance was routed to `VISIBILITY_LAYER_EFFECT` (32), which is outside `VISIBILITY_MASK_ALL_OPAQUE` (15) and `VISIBILITY_MASK_SOLID` (31). The commit removed that divert as part of the single-sided-wall light-leak fix (Bleak Falls Barrow ice and door panels), so a blended non-actor now keeps its render layer's opaque bucket. The new comment gives the premise:

> True non-occluders are separated by MATERIAL KIND above (effect shader, fire refraction, refractive glass) — the authored "not solid" signal; blend state is not.

That holds for Skyrim+, whose effect family imports as `MATERIAL_KIND_EFFECT_SHADER` (101). It does not hold for:
- **FO3/FNV.** The effect family is `BSShaderNoLightingProperty`, which imports as `MATERIAL_KIND_NO_LIGHTING` (102) (`crates/nif/src/import/material/legacy_properties.rs`, pinned by `nolighting_sets_material_kind_to_102`). Kind 102 is not in the non-occluder arm.
- **Oblivion.** FX cards import as kind 0.

TLAS membership (`byroredux/src/render/static_meshes.rs` `tlas_exclusion`) excludes only distant LOD blocks, `IsDecalMesh` and fire refraction. These cards are therefore in the TLAS, and now sit in the `ARCHITECTURE` / `STATIC_PROP` / `FOLIAGE` buckets.

## Evidence

- **Code (HEAD).** `shadow_mask_for_instance` tests glass → `EFFECT_SHADER`/`FIRE_REFRACTION` → actor → `match render_layer`. It has no kind-102 arm and no blend arm, and `_alpha_blend` is unused.
- **Coverage.** `rayHitHasCoverage` treats a blended non-glass hit as covered at alpha ≥ 1/255. Without `INSTANCE_FLAG_DIFFUSE_ALPHA` and with `alphaThreshold == 0`, alpha is forced to 1.0, so soft fog and beam textures are effectively solid quads. `traceShadowBinary` (`gl_RayFlagsOpaqueEXT | gl_RayFlagsTerminateOnFirstHitEXT`) has no coverage test at all.
- **Real-data census.** The audit ran a read-only scratch tool over the vanilla `Meshes` BSAs. It counted drawn meshes that are alpha-blended, not glass-keyword, not kind 100/101/103, and not a never-drawn `IsFxMesh` texture:
  - **FNV: 4,699** (kind 102: 3,777). Examples: `effects\nv\hchambergroundfog.nif`, `effects\ambient\fxmistlow01.nif`, `architecture\strip\lucky38lights.nif`, `ultraluxdome_glowsbackside.nif`.
  - **FO3: 2,119** (kind 102: 1,613). Examples: `clutter\fakefog01.nif`, `effects\ppurityfx\ppurityfxtankfog01.nif`.
  - **Oblivion: 1,011** (kind 0). Examples: `dungeons\misc\fx\fxlightbeam01.nif`, `oblivion\environment\fxoblivionlightbeam01.nif`, `oblivion\gate\flashglow01.nif`.
  - Nearly all are additive `SRC_ALPHA/ONE` or soft-alpha cards.
- **Lights affected.** FO3/FNV ESM lights are all `FULL` (the zero-authoring correction), as are NIF lights and the sun/XCLL. Oblivion's unflagged lights use the conservative mask, which gained `STATIC_PROP` in `d54382415`.

## Impact

- Ground-fog and mist planes shadow the floor beneath them from every overhead light.
- Light-beam and glow shells block the lights they depict and cast hard quad shadows.
- Volumetric in-scatter is cut off at each card (`traceShadowBinary`).
- Smoke and fire transport stops at mist and beam cards. `combustionPathBlocked` traces `VISIBILITY_MASK_SOLID`, so the comment above it ("effect cards do neither") is now false for these cards.
- `rt.masks` cannot show any of this, because blended cards are now counted inside the opaque buckets.
- The per-cell magnitude has not been captured live (see the report's "Needs-RenderDoc / live validation").

## Related

- #3305 (open): actor ground-contact shadows, the opposite direction of the same mask policy.
- `84bbc44ed`: the blended-actor carve-out, which `f97775ca8` keeps.
- REN-D1-2026-09-21-02 (#4580): `TRIANGLE_FACING_CULL_DISABLE` is inert. The `f97775ca8` investigation relied on the opposite premise.
- REN-D8-2026-09-21-02 (#4589): the water-caustic visibility ray's mask. Now that legacy FX cards sit in opaque buckets, no mask choice can exclude them.
- Cited, not re-reported, by `docs/audits/AUDIT_PERFORMANCE_2026-09-21.md`.

## Suggested Fix

Keep the Skyrim ice/door-panel intent, and add the legacy authored "not solid" signals to the non-occluder arm: `MATERIAL_KIND_NO_LIGHTING` (102) and additive blends (`dst_blend == ONE`). The durable version is an explicit canonical non-occluder bit carried across the NIFAL boundary, so the renderer does not re-derive it from per-game kinds. Also:
- add an FNV fixture and an Oblivion fixture to `shadow_mask_bucket_selection_is_pinned`;
- add a blended-in-opaque-bucket counter to `rt.masks`.

Validate with an A/B pre/post `f97775ca8` (`rt.masks` + screenshots) on one cell per game: FNV Lucky 38, FO3 Project Purity, and an Oblivion Ayleid beam dungeon. Include Bleak Falls Barrow, the case that motivated the commit.

Source: docs/audits/AUDIT_RENDERER_2026-09-21.md (REN-D1-2026-09-21-01)

## Completeness Checks
- [ ] **SIBLING**: `mask_divert_cause` and the `rt.masks` census partition stay in lockstep with the new arm (`divert_cause_matches_the_mask_it_explains`)
- [ ] **SIBLING**: every mask consumer re-checked — `traceShadowTransmittanceDetailed`, `traceShadowBinary` (volumetrics_inject / caustic_splat), `combustionPathBlocked`, and `water.frag`'s caustic visibility ray
- [ ] **CANONICAL-BOUNDARY**: if a non-occluder bit is added, it is derived once at the NIFAL parser→`Material` boundary (`translate_material`), never re-derived per game in the renderer
- [ ] **TESTS**: FNV (kind-102 additive card) and Oblivion (kind-0 blended card) fixtures in `shadow_mask_bucket_selection_is_pinned`; the Bleak Falls Barrow ice-panel case stays in its opaque bucket
