# NIFAL Audit — 2026-09-16

`/audit-nifal --focus 1,8`, run as part of the `texture-roles-deep` audit-suite
preset. It covers two dimensions: Dimension 1 (the material boundary and its
narrowed signature) and Dimension 8 (shader flags and the texture-role
vocabulary). All game variants were in scope. Live tree at HEAD `7996edf61`.

Baseline: `docs/audits/AUDIT_NIFAL_2026-09-14.md` (HEAD `7374634f5`). Every
commit since then that touches a Dimension 1 or 8 entry point was reviewed. That
is 90 commits in total, of which these matter here:
- `2bdd36369` (#4391) and `c9447bd94` (#4393): fixes for two of the prior HIGH findings.
- `c9d4f007c`: flipped normal maps are now gated on their own alpha.
- `71336643f` (#4302): the boundary-material guard now walks the whole spawner tree.
- `60cef1c64`: adds `resolve_linear_texture`.
- `e3131f5ef`: merge and CDB cleanup.
- `eaa94b49d`: splits the shader family into `crates/nif/src/blocks/shader/`.
- the `triangle.frag` changes.

All work was done in this session, with no sub-agents. Every finding was
re-checked against current code. Where content was needed as ground truth, it
was measured against the vanilla archives by two throwaway census programs,
built outside the repo and linked against `byroredux-nif`, `byroredux-bsa` and
`byroredux-bgsm`. No source was modified.

## Executive Summary

**6 new findings: 0 CRITICAL, 0 HIGH, 2 MEDIUM, 4 LOW.** Nothing regressed in
the Dimension 1 material boundary. The two HIGH findings from 2026-09-14 in these
dimensions are fixed and their fixes hold:
- **#4391**: the window/eye env-mapping glass signal now requires blended coverage.
- **#4393**: an unauthored Skyrim effect-shader `env_map_scale` no longer enters `MaterialInfo`, and the completeness harness now has a PBR-override ceiling.

**Headline: `NiFlipController` flipbooks are a second route from a canonical
texture role to a bindless handle, and that route bypasses the per-role
resolver.** #3901 correctly turned the flipbook's raw `TexType` into a
`FlipTextureRole`. The frames behind that role are still resolved by plain
`resolve_texture`. That skips the three decisions `resolve_material_texture_handles_with_clamp`
makes per role:
- **Sampler address mode.** The flipbook uses the default WRAP instead of the material's authored clamp mode. This is live on 9 vanilla Oblivion-gate flip controllers that author CLAMP.
- **Colour space.** Every frame is uploaded as sRGB, including Normal/Height/SmoothSpec flips. No vanilla content hits this today.
- **Missing-frame fallback.** A missing frame shows the magenta checker instead of handle 0.

The same frames are also left out of the texture-release lifecycle: every
acquired refcount survives cell unload (D8-02).

The other new findings:
- **D8-03**: FO4's slot-2 routing is the only slot arm with no cited evidence. It ignores the `Glow_Map` gate it is given, and the BGSM `glowmap` flag is parsed but never read. A census shows the NIF flag is unreliable in both directions, so this needs a source, not a one-line fix.
- **D8-04, D8-05, D8-06**: vocabulary and guard hygiene.

Per-category status for this sweep's scope, against `docs/engine/nifal.md` §2:

| Category | Spec status | This sweep |
|---|---|---|
| Material (Dim 1) | converged | **converged**: 0 new findings. #4391 and #4393 fixed and verified. Open, unchanged: #4392, #4400, #4256, #4246 |
| Shader flags / texture sets (Dim 8) | converged | **1 new side path**: the flipbook frame resolver (2 MEDIUM). 1 LOW measured FO4 ambiguity, 3 LOW hygiene. Zero per-game branches in `crates/renderer/shaders/triangle.frag` and its includes |

Tier-invariant violations among the new findings:

| Invariant | Count | Findings |
|---|---|---|
| single-boundary | 3 | D8-01, D8-02, D8-04 |
| no-fabrication | 1 | D8-03 (FO4 slot-2 arm: an unmeasured routing rule) |
| no-leak | 0 | — |
| no-render-time-fallback | 0 | — |
| harness-coverage / doc | 2 | D8-05, D8-06 |

## Per-Category Tier Matrix

| Category | single-boundary | no-fabrication | no-leak | no-render-time-fallback | Boundary fn |
|---|---|---|---|---|---|
| Material | PASS. Callers: `byroredux/src/scene/nif_loader.rs`, `byroredux/src/cell_loader/spawn/mesh_instance.rs`, `byroredux/src/cell_loader/placement_lod.rs`, `byroredux/src/cell_loader/object_lod.rs`, plus the Cornell harness. Signature is still `&ImportedMaterial` + `mesh_name`. `merge_external_material` is still `&mut ImportedMaterial` and pinned as the sole export. The #4400 MSWP seed split is still open | PASS on new code (#4391/#4393 fixed). Existing: #4392 | PASS. `Material.metalness`/`roughness` are plain `f32`, clamped to `[0,1]` / `[0.04,1]` in `resolve_pbr` | PASS. The only `classify_pbr` mention in `byroredux/src/render/` is a comment | `byroredux/src/material_translate.rs::translate_material`. `resolve_pbr` runs before `classify_glass_into_material` |
| Shader flags / texture sets | **FAIL**: D8-01/D8-02. Flipbook frames bypass `resolve_material_texture_handles_with_clamp` and the `secondary_values()` release walk. D8-04: two slot tables disagree on Starfield | **PARTIAL**: D8-03. The FO4 slot-2 arm is uncited | PASS. No per-game slot index crosses the import boundary; the flipbook carries `FlipTextureRole` | PASS. No `game ==` branch or per-game identifier in `crates/renderer/shaders/triangle.frag` or `crates/renderer/shaders/include/` | `crates/nif/src/import/material/slot_role.rs::slot_to_role` → `MaterialTextureSet<T>` (26 entries; `roles`/`map_ref`/`zip_map_ref`/`values` agree; guards green) → `byroredux/src/asset_provider/texture.rs::resolve_material_texture_handles_with_clamp` |

## Findings

### MEDIUM

#### NIFAL-D8-2026-09-16-01: Flipbook frames bypass the per-role texture resolver — authored CLAMP is dropped on the vanilla Oblivion gates, and Normal/Height/SmoothSpec flips would upload as sRGB
- **Severity**: MEDIUM. Rendering is wrong on 9 vanilla flip controllers, but the only visible effect is at texture edges. The colour-space and fallback halves are latent on vanilla content.
- **Dimension**: Shader-flags/Effects (texture roles)
- **Tier Violated**: single-boundary
- **Game Affected**: Oblivion (live, clamp mode). Any game with a mod-authored non-base flipbook (colour space, fallback).
- **Location**: `byroredux/src/anim_convert.rs:246-253` (frame resolve). Compare with `byroredux/src/asset_provider/texture.rs:712-781` (`resolve_material_texture_handles_with_clamp` / `map_secondary_texture_handles`). Consumer: `byroredux/src/render/static_meshes.rs:158-195` (`apply_texture_flip_roles`) and `:447-452` (base-color flip replaces `TextureHandle`).
- **Status**: NEW. The resolve line dates from #2221 (`7fbc5bafb`). #3901 (`4520f8d53`) made every non-base role live through it.
- **Description**: Static material textures reach the GPU through one per-role resolver. `resolve_material_texture_handles_with_clamp` decides three things for each role:
  1. **Sampler address mode**: the material's authored `texture_clamp_mode`. Its doc says the walk exists so that "structures, clutter, actors, and exterior statics cannot drift".
  2. **Colour space**: `map_secondary_texture_handles` uploads `normal`, `smooth_spec` and `height` as Linear, and `emissive`, `dark`, `detail` and the decals as sRGB.
  3. **Missing texture**: a secondary role that is authored but missing collapses to `0`, so the shader treats it as absent instead of sampling the magenta fallback.

  The flipbook resolves the frames that replace those same roles with `crate::asset_provider::resolve_texture(ctx, tex_provider, Some(path))`. That call fixes all three to REPEAT (3), sRGB and the checker fallback, whatever the role. After #3901, `apply_texture_flip_roles` writes those handles straight into `MaterialTextureSet<u32>` slots that the static resolver had filled under different rules.
- **Evidence**:
  - `byroredux/src/anim_convert.rs:252`: `.map(|path| crate::asset_provider::resolve_texture(ctx, tex_provider, Some(path)))`. `resolve_texture` is `resolve_texture_with_clamp(.., 3)` → `TextureColorSpace::Srgb` (`byroredux/src/asset_provider/texture.rs:509-519`, `:547-560`).
  - The static paths resolve the base with `resolve_texture_with_clamp(.., material.texture_clamp_mode)` (`byroredux/src/cell_loader/spawn/mesh_instance.rs:933`, `byroredux/src/scene/nif_loader.rs:1141`) and the secondary roles with the per-role table.
  - **Census** (all 9 Oblivion mesh BSAs, every `NiFlipController` joined to its host `NiTexturingProperty` through the controller chain), as a histogram of `(TexType, authored clamp of that slot's TexDesc)`: `(0 BASE, 3 WRAP) × 49`, **`(0 BASE, 0 CLAMP_S_CLAMP_T) × 9`**, `(4 GLOW, 3) × 1`, and 1 host not resolved (`magiceffects\shockshield.nif`).
  - The 9 CLAMP controllers are the 16-frame portal flipbooks in `meshes\oblivion\gate\obliviongate_forming.nif`, `oblivionarchgate01.nif` and `obliviongate_simple.nif` (3 each). Those are the Oblivion Gate meshes placed throughout the Tamriel worldspace.
  - The flipped handle replaces `TextureHandle` on every frame (`byroredux/src/render/static_meshes.rs:451-452`), so the WRAP-sampled frame is what is always drawn. The material's CLAMP applies only to the unused spawn-time base handle.
- **Impact**:
  - **Oblivion gates**: the portal surface samples with REPEAT where the art authors CLAMP. Bilinear filtering blends in opposite-edge texels along UV borders, the edge bleed #610 exists to prevent. Each frame path is also cached twice, under `(path, 3)` for the flip and `(path, 0)` for the static base (a small duplicate VRAM cost).
  - **Latent**: a flipbook on `Normal`, `Height` or `SmoothSpec` (reachable since #3901, which measured 0 vanilla instances) would upload through the sRGB curve. A flat 125/255 normal texel would decode to about 0.21 instead of 0.49, which is exactly the failure `resolve_linear_texture`'s new doc (`60cef1c64`) describes.
  - **Latent**: a missing non-base frame binds the magenta checker into a secondary role, where the static path binds `0`.
- **Related**: #3901, #4301, #2221, #610, #3516, NIFAL-D8-2026-09-16-02
- **Suggested Fix**:
  1. Expose a single-role entry point from `map_secondary_texture_handles`'s table, a `(role → cubemap, colour space)` lookup keyed on `FlipTextureRole`.
  2. Resolve flip frames through it, using the target entity's `Material.texture_clamp_mode` (already inserted when `attach_animation_sinks` runs) and the same fallback-to-0 rule for non-base roles.
  3. Pin it with a test that drives a `Normal` flip and a CLAMP base flip through the attach path.

#### NIFAL-D8-2026-09-16-02: Flipbook frame handles are acquired with a registry refcount that no unload path ever releases
- **Severity**: MEDIUM. This is a resource leak that grows with every cell load (not every frame). The resident VRAM is bounded by the number of distinct flipbook textures.
- **Dimension**: Shader-flags/Effects (texture-role lifecycle)
- **Tier Violated**: single-boundary. `MaterialTextureSet::values()` / `secondary_values()` is the declared exhaustive lifecycle walk for role textures, and flip frames are role textures held outside it.
- **Game Affected**: Oblivion (60 vanilla flip controllers). Any content with `NiFlipController`.
- **Location**: `byroredux/src/anim_convert.rs:246-253` (acquire). `byroredux/src/cell_loader/unload.rs:494-531` (release walk covers `TextureHandle`, `NormalMapHandle`, `WaterNoiseMapHandles`, `MaterialTextureHandles`, but not `AnimatedTextureFlip`). The cell attach site is `byroredux/src/cell_loader/spawn.rs:833-843`.
- **Status**: NEW (present since #2221, `7fbc5bafb`)
- **Description**: `resolve_texture_view_with_clamp` deliberately uses `acquire_by_path_*`, so "each resolve pairs with one drop_texture on cell unload" (#524). The flipbook path calls it once per frame per placement and stores the handles in `TextureFlipEntry.handles` on the `AnimatedTextureFlip` component. `collect_unload_drops` never queries `AnimatedTextureFlip`, and `grep drop_texture` finds no other consumer of those handles. Every acquired reference therefore outlives the entities that hold it.
- **Evidence**:
  - `byroredux/src/cell_loader/unload.rs:494-499` lists the six queried component types; `AnimatedTextureFlip` is not one of them.
  - The cell path passes `Some(ctx)` / `Some(tex_provider)` into `attach_animation_sinks` for every placement (`byroredux/src/cell_loader/spawn.rs:833-843`), so each placed Oblivion gate acquires 3 × 16 = 48 references per cell load.
  - The census from D8-01 finds 60 flip controllers in the vanilla Oblivion archives (up to 16 frames each).
- **Impact**:
  - Flipbook textures (Oblivion Gate portals, fire and water flipbooks) are never freed once a cell that places them has loaded. Their refcounts grow by the frame count on every re-entry.
  - The registry's deferred-destroy backlog, `texture_pending_destroy_count` (published since `e3131f5ef`), cannot see this because nothing is ever queued.
  - Long exterior sessions keep every flipbook texture ever seen resident.
- **Related**: #524, #2221, #3901, #4117, NIFAL-D8-2026-09-16-01
- **Suggested Fix**:
  1. Add an `AnimatedTextureFlip` arm to `collect_unload_drops` that pushes every `handles` entry through `push_tex_drop`.
  2. Better, give `AnimatedTextureFlip` a `values()`-style walk and extend the unload test that pins `MaterialTextureHandles` release, so the next flipbook-held handle cannot be missed.

### LOW

#### NIFAL-D8-2026-09-16-03: FO4 is the only slot-2 arm that ignores the `Glow_Map` gate it is handed, and the BGSM `glowmap` flag is parsed but never read — the census shows the NIF flag is unreliable in both directions, so the rule needs a source
- **Severity**: LOW. The right answer is unknown without an FO4 reference. At most 20 vanilla properties are plausibly wrong today (see Evidence).
- **Dimension**: Shader-flags/Effects (texture roles)
- **Tier Violated**: no-fabrication. It is the only `slot_to_role` arm with no cited measurement, and its companion comment is false.
- **Game Affected**: FO4
- **Location**:
  - The routing arm: `crates/nif/src/import/material/slot_role.rs:361-367`, `(Fallout4, 2)`, which returns `Emissive` for every non-tint type and never reads `context.glow_map`.
  - The false comment: `crates/nif/src/import/material/dedicated_shader.rs:324-326`, "The `Glow_Map` bit (F4SF2 bit 6) participates only in the texture-slot vocabulary above". The bit is computed at `:112-127` and placed in `TextureSlotContext.glow_map`, but the FO4 arm discards it.
  - The flag parsed and never read: `crates/bgsm/src/bgsm.rs:121` and `:292`. The only readers are the parser and its tests.
  - The unconditional BGSM fill: `byroredux/src/asset_provider/material/merge.rs:614-619`.
- **Status**: NEW. #3068 gated Skyrim in `86c410229` and gave no FO4 evidence. #1733 looked only at the flag-set-but-no-texture case and called it inert.
- **Description**: nif.xml defines slot 2 as `Glow(SLSF2_Glow_Map)/…` (`nif.xml:6313`), and `Fallout4ShaderPropertyFlags2` has `Glow_Map` at bit 6 (`:6487`). The other slot-2 arms follow that gate: Skyrim tests the flag (#3068), and FO76/Starfield test the CRC `GLOWMAP`. The FO4 arm binds any non-empty slot 2 as the emissive mask. On the external side, BGSM v≤2 carries its own `glowmap` bool next to `glow_texture`, and the merge fills `emissive` from `glow_texture` without looking at the bool. `triangle.frag` replaces `emissiveMask` with the glow sample whenever `glowMapIndex != 0`, so the role decides whether emission is masked or flat.
- **Evidence**: census of `Fallout4 - Meshes.ba2` + `Fallout4 - MeshesExtra.ba2`, non-tint `BSLightingShaderProperty` with slot 2 populated:

  | Glow_Map flag | emissive authored | count |
  |---|---|---|
  | clear | no | 2,493 (inert) |
  | clear | **yes** | **137** — 116 with a `_g.dds` glow map (e.g. `putridglowingonebodyb.nif` → `GlowingOneHead1_g.DDS`, `mirelurkqueen.nif` → `MeatTile01_g.DDS`); **20 with a `_d.dds` diffuse** (e.g. `terminaloninstitute.nif` → `PipBoyScreen_d.dds`, `floorlampnoshadeonoff.nif` → `lightfixtureglass01_d.dds`); 1 `ColorWhiteUtility` |
  | set | no / yes | 3,254 / 5,951 |

  `Fallout4 - Materials.ba2` + the three DLC mains, BGSM v≤2 with `glow_texture` non-empty: `glowmap=false` × 65 (5 of them `emit_enabled`, e.g. `bloodbugremap.bgsm`, `sublightinner.bgsm`), `glowmap=true` × 249.

  So neither reading is safe. Gating on the NIF flag, as Skyrim does, would strip the mask from 116 genuine FO4 glow maps, Glowing Ones among them. Routing without the gate, as today, masks emission with a diffuse texture on 20 properties.
- **Impact**: For up to 20 inline FO4 properties (Institute terminal screens, lit floor lamps), emission is shaped by a diffuse texture where the engine may emit flat colour. The 5 BGSM `glowmap=false` + `emit_enabled` materials are in the same situation. The bigger cost is that this is the one arm in the table with no recorded evidence, sitting next to a comment that claims the gate is used. That is the pattern that let #3068 go unquestioned.
- **Related**: #3068, #1733, #1592, #2997/#2998/#2999, #3458
- **Suggested Fix**:
  1. Do not flip the gate yet. First find a source for FO4's actual rule (does the lit shader sample slot 2 without `Glow_Map`, and does BGSM `glowmap` override the NIF bit?).
  2. Correct the `crates/nif/src/import/material/dedicated_shader.rs:324-326` comment, and record the census above in the FO4 arm, the way every other arm records its numbers.
  3. Once the source is in hand, decide whether `glowmap` gates the BGSM `emissive` fill.

#### NIFAL-D8-2026-09-16-04: `slot_to_colocated_role` still groups Starfield with Skyrim after #3900 moved Starfield to FO76's slot vocabulary
- **Severity**: LOW (the arm is unreachable today)
- **Dimension**: Shader-flags/Effects (texture roles)
- **Tier Violated**: single-boundary (one game, two slot vocabularies inside one module)
- **Game Affected**: Starfield (latent)
- **Location**: `crates/nif/src/import/material/slot_role.rs:276-286` (`(TextureSlotLayout::Skyrim | TextureSlotLayout::Starfield, 2)`); compare with `:288-328` (#3900 doc) and `:368-376`
- **Status**: NEW. The arm dates from `d5a8c36c0` (#3458, 2026-08-28). #3900 (`c5f35544c`, 2026-09-07) moved `slot_to_role` and did not update this sibling.
- **Description**: #3900's stated rule is that one canonical boundary must not hold "two rival vocabularies for one game", and it moved Starfield onto FO76's arms. `slot_to_colocated_role` was not moved: for Starfield it still returns `LightingMask` on tint-family slot 2 when soft/rim lighting is set, while FO76 returns `None`. The arm cannot fire today:
  - `apply_bs_lighting_shader` computes `soft_lighting`/`rim_lighting` only for the Skyrim layout (`crates/nif/src/import/material/dedicated_shader.rs:169-181`).
  - #3900's census found 0 Starfield `BSShaderTextureSet` blocks.

  The REFR overlay also calls this function (`byroredux/src/cell_loader/spawn/mesh_instance.rs:326-329`).
- **Evidence**: see the match arm at `:278`. The module's own test (`:645-651`) asserts only the FO4 `None`.
- **Impact**: None on current content. A future Starfield soft-lighting capture would silently take the Skyrim co-location, the exact drift #3900 was meant to close.
- **Related**: #3900, #3458, #3732, #2695
- **Suggested Fix**: Drop `Starfield` from the colocated arm (or group it with `Fallout76`), and extend the `:645` test to cover every non-Skyrim layout.

#### NIFAL-D8-2026-09-16-05: The per-role colour-space guard pins 13 of the 25 secondary roles — `wrinkle`, `flow`, `lighting` and `reflectance` (all data maps) could flip to sRGB with no failing test
- **Severity**: LOW (test gap; the current table looks deliberate)
- **Dimension**: Shader-flags/Effects (texture roles)
- **Tier Violated**: harness-coverage
- **Game Affected**: FO4/FO76 (wrinkle, flow, lighting, reflectance), all (tint, inner layer, decals)
- **Location**: `byroredux/src/asset_provider/texture.rs:906-986` (`common_material_texture_walk_covers_every_secondary_role_once`); table at `:743-782`
- **Status**: NEW
- **Description**: The test checks that every secondary role is visited once and that only `environment` is a cubemap. Colour space is asserted for only 7 Linear roles (`normal`, `smooth_spec`, `height`, `environment_mask`, `specular`, `lighting_mask`, `glass_roughness_scratch`) and 6 sRGB roles (`emissive`, `detail`, `dark`, `back_lighting`, `glass_dirt_overlay`, `decal_0`). `tint`, `inner_layer`, `lighting`, `flow`, `wrinkle`, `greyscale_lut`, `reflectance`, `emittance_gradient` and `decal_1..3` are unpinned.
  - `wrinkle` is an `_n` normal map (#2999) and `flow` is a vector field. `resolve_linear_texture`'s new doc names both as data textures.
  - #3814 already fixed the same shape of gap for the GPU lanes, so this is the remaining unpinned half.
- **Evidence**: the two `for role in [...]` lists at `:958-986`.
- **Impact**: A role-table edit that swaps a data map to sRGB would silently corrupt wrinkle normals or flow vectors on FO4 heads and water.
- **Related**: #3814, #2999, `60cef1c64`
- **Suggested Fix**: Assert the full 25-role colour-space table (exhaustive match on role name), so that adding a role without choosing its colour space fails the test.

#### NIFAL-D8-2026-09-16-06: `MaterialInfo::texture_set` says inline NIF shaders never expose the standalone `specular` role, which has been false since #2998/#3085
- **Severity**: LOW (doc)
- **Dimension**: Shader-flags/Effects (texture roles)
- **Tier Violated**: — (doc)
- **Game Affected**: FO4, FO76, Skyrim (model-space-normal slot 7)
- **Location**: `crates/nif/src/import/material/mod.rs:1332-1334`
- **Status**: NEW
- **Description**: The comment "Standalone BGSM/BGEM roles are populated by the downstream material-file translator; inline NIF shaders do not expose them." sits above `specular: self.specular_map`. It dates from `1d94eb24` and predates `79202bfc4`. `slot_to_role` now routes FO4 slot 7, FO76 slot 6 and Skyrim MSN slot 7 into `TextureRole::Specular`, and `apply_bs_lighting_shader` writes those into `info.specular_map` (`crates/nif/src/import/material/dedicated_shader.rs:260`).
- **Evidence**: `git blame` shows the comment from `1d94eb246` and the `specular` line from `79202bfc4`.
- **Impact**: A reader auditing role provenance would conclude that an inline `specular` value must be a BGSM leak, the wrong premise for the `smooth_spec`/`specular` mis-merge check this dimension exists to make.
- **Related**: #2998, #3085, #2742
- **Suggested Fix**: Move the comment to the roles it still describes (`flow`, `glass_*`), and note that `specular` has inline NIF producers.

## Regression pins verified (do not re-file)

- **Dimension 1 signature and boundary**:
  - `translate_material(source: &ImportedMaterial, mesh_name: Option<&str>, paths: ResolvedPaths, extra_material_flags: u32) -> Material`; not widened back to `&ImportedMesh`.
  - `merge_external_material(&mut ImportedMaterial, …)`, pinned by `merge_external_material_is_the_only_exported_fn_in_this_file`.
  - `resolve_pbr` clamps metalness to `[0,1]` and roughness to `[0.04,1]` and reads `self.specular_authored` (#2573).
  - `classify_glass_into_material` runs after `resolve_pbr` (`byroredux/src/material_translate.rs:760-761`).
  - No `metalness_override`/`roughness_override` field on the canonical `Material`, and no render-time `classify_pbr`.
- **#4391** (`2bdd36369`): `source.window_env_mapping && source.has_alpha` at the call site. The translate-level test `window_env_mapping_promotes_to_glass_only_with_blended_coverage` is green.
- **#4393** (`c9447bd94`): the effect-shader `env_map_scale` copy and its latch are gated on `bsver >= FALLOUT4`. `assert_pbr_override_ceiling` exists in `crates/nif/tests/translation_completeness.rs`.
- **Texture-role walks**:
  - `roles()`, `map_ref`, `zip_map_ref` and `values()` each list the same 22 roles plus 4 decals in the same order, with `base_color` first, so `secondary_values()`' `skip(1)` is still correct.
  - Guards present and green: `values_covers_every_field_in_the_set`, `roles_covers_every_field_in_the_set`, `documented_texture_role_list_matches_the_struct`, and the #3814 lane guard in `byroredux/src/render/static_meshes.rs`.
- **#3901**: no raw `TexType` on a canonical component; `flip_role_from_tex_type` follows `NiTexturingProperty`, not `slot_to_role`. The residual resolver bypass is D8-01.
- **`smooth_spec` vs `specular`, `environment` vs `environment_mask`**: kept distinct at every stage.
  - NIF: `gloss_map → smooth_spec`, `specular_map → specular`.
  - BGSM: `smooth_spec_texture → smooth_spec`, `specular_texture → specular`, `envmap → environment`.
  - BGEM: `envmap`/`envmap_mask` → their own roles.
  - REFR overlay: `smooth_spec` and `external_specular` are separate fields.
  - GPU: `gloss_map_index` vs `slot::SPECULAR`, `env_map_index` vs `env_mask_index`.
- **No per-game shader branch**: `crates/renderer/shaders/triangle.frag` and every include under `crates/renderer/shaders/include/` contain no `game ==` test and no game-name identifier outside comments, including the ground-cover, sky and water changes since 09-14.
- **FO4 MSN + Alpha_Test (#1592)** and **lit-path palette enable bits (#3897)**: both still captured in `apply_bs_lighting_shader`.
- **No `Mat` provenance arm** (#3906): `ImportedTextureSource` is still `NifTextureSet`/`Bgsm`/`Bgem`.

## Existing open issues re-checked (unchanged, not re-filed)

- **#4392**: `lit_carrier_authored_dispatch` still at `byroredux/src/helpers.rs:119`.
- **#4400**: `textures` is still seeded from `mesh.material.textures` at `byroredux/src/cell_loader/spawn/mesh_instance.rs:189-192`. The re-check found only `glass_roughness_scratch`/`glass_dirt_overlay` affected: `reflectance`, `emittance_gradient`, `dark` and the decals have no BGSM/BGEM producer, so the swap cannot change them.
- **#4401**, **#4402**, **#4403**: no code change since filing.
- **#4256** (`shader_type` not on `Material`) and **#4246** (two-phase doc and the `byroredux/src/cell_loader/object_lod.rs` exemption): still open.
- **#4282**, **#4283**: Starfield lighting-shader scalars and `from_bgsm` overload. Not touched this window.

## Documented-limitation ledger

- **Emissive scale is a deliberate no-op** (`docs/engine/nifal.md` §4). `BSEffectShaderProperty.base_color_scale` is routed through `EmissiveSource::Effect`, pending an effect render path.
- **`material_kind: u32`** is the GPU dispatch contract (0–20 vanilla, 100 GLASS, 101 EFFECT_SHADER), not a leak.
- **Starfield `.mat`/CDB** is not a live role source (Phase 1 header probe only); the #3398 Phase 2 is open.
- **BGSM `distance_field_alpha_texture`** (v≥17) has no role (#2642). This is a deferred-consumer gap.
- **Skyrim/FO76 tint-family slots 4/5** are deliberately skipped (#1350). FO4 slots 4/5 route to Environment/Wrinkle (#2999).
- **FaceTint slot 6** is deliberately unrouted; it is owned by FaceGen (#2095).
- **Starfield `slot_to_role` arms** are a defensive default: shipped content has 0 texture sets (#3900).

## Method notes

- **Code review**: the entry points of both dimensions were read at HEAD. For each commit since `7374634f5` touching those files, the `git show`/diff was reviewed.
- **Censuses** (scratch programs outside the repo, release build):
  1. **Oblivion flip controllers**: `NiFlipController` joined to its host `NiTexturingProperty` through the controller chain, across `Oblivion - Meshes.bsa`, `DLCShiveringIsles - Meshes.bsa`, `Knights.bsa` and the six small DLC BSAs. Result: 60 controllers, histogram in D8-01.
  2. **FO4 slot 2**: `BSLightingShaderProperty` slot-2 occupancy × `F4SF2::Glow_Map` × authored emissive × slot-2 filename suffix across `Fallout4 - Meshes.ba2` + `Fallout4 - MeshesExtra.ba2`, plus BGSM `glow_texture` × `glowmap` × `emit_enabled` across `Fallout4 - Materials.ba2` and the Coast, NukaWorld and Robot mains. Results in D8-03.
- **Tests executed** (all green):
  - `cargo test -p byroredux-nif --lib -- slot_role material_texture_set values_covers roles_covers`: 22 passed.
  - `cargo test -p byroredux --bin byroredux -- material_translate common_material_texture_walk apply_texture_flip glass_classification`: 91 passed.
- **Not run**: the `#[ignore]` completeness harness (Dimension 9 scope). The 09-14 numbers plus #4393's re-measurement stand.
- **Dedup**: `gh issue list` (200 most recent) plus a full-state `gh issue list --search` for `flip` and `glow`. No existing issue covers D8-01..06. The flip-leak and flip-resolver findings have no match. #3068 (Skyrim) and #1733 (FO4 flag-without-texture) are related but closed with different scope.
- **Validate gate**: `.claude/commands/_audit-validate.sh` **FAILS at HEAD, but not because of this report.** It reports 10 STALE references to *crates/nif/src/blocks/shader.rs*, a file that `eaa94b49d` (2026-09-15) split into `crates/nif/src/blocks/shader/` (`mod.rs`, `lighting.rs`, `effect.rs`, `legacy.rs`, `sky_water.rs`). The stale references sit in `.claude/commands/audit-fo3/SKILL.md`, `.claude/commands/audit-fo4/SKILL.md`, `.claude/commands/audit-nif/SKILL.md` (×2), `.claude/commands/audit-skyrim/SKILL.md` (×3), `.claude/commands/audit-starfield/SKILL.md` (×2) and `.claude/commands/audit-tech-debt/SKILL.md`. The gate does not scan `docs/audits/`. Every backticked path in this report was checked separately against the live tree, and all resolve.
- **Out-of-scope observations** (route to `/audit-tech-debt`, not NIFAL findings):
  - The same split left code comments pointing at the old file, e.g. `crates/nif/src/import/material/mod.rs:950` ("See `BSWaterShaderProperty` in …/blocks/shader.rs"). A `grep -rn 'blocks/shader.rs'` over `crates/` and `byroredux/` will find the rest.
  - The throwaway census examples (crates/nif/examples/tmp_fo4_d4_{psglod,lodoverlap,lodsize}.rs) are still committed (already noted on 09-14).

**Suggested publish labels**: domain `nifal` on all findings.
- **D8-01**: `renderer` + `animation` + `game:oblivion`.
- **D8-02**: `memory` + `renderer` + `game:oblivion`.
- **D8-03**: `import-pipeline` + `game:fo4`.
- **D8-04**: `nif-parser` + `game:starfield`.
- **D8-05**: `test-gap`.
- **D8-06**: `doc-rot`.

## Next Step

```
/audit-publish docs/audits/AUDIT_NIFAL_2026-09-16.md
```
