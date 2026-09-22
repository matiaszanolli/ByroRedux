=== #4409 ===
# null: NIFAL-D5-2026-09-14-03: `docs/engine/nifal.md` §2 Particles no longer describes what the boundary applies or defers after #3754/#4240/#4261 [OPEN]

**Source**: `docs/audits/AUDIT_NIFAL_2026-09-14.md` (`/audit-nifal`, HEAD `7374634f5`)

- **Severity**: LOW (doc; the spec is the authority auditors check "parked" against)
- **Dimension**: Particles
- **Tier Violated**: parked-not-leak
- **Game Affected**: all
- **Location**: `docs/engine/nifal.md:301-303`, `:311-317`, `:335-338`
- **Status**: NEW (precedent #2488)
- **Description**:
  - (a) The applied-field list omits `planar_angle` / `planar_angle_variation` (#4240) and the ×2 half-spread→full-width variation convention.
  - (b) The rate is still described as "`NiFloatData` first key"; the #3754 curve-mean, #2548 blend and #3329 sequence tiers are missing.
  - (c) Per-emitter attribution is still called wholly pending, although #4261 made params, colour, budget and the modern rate tier per-instance; only the legacy and #3329 sequence tiers remain whole-scene.
  - (d) The tooling line omits planar columns.
- **Evidence**: Line citations against `9e372f452` / `b3237e65a`; neither commit touched nifal.md.
- **Impact**: A later audit will misclassify the sequence-tier residual and miss the variation-convention change that affects fog-volume sizing (`byroredux/src/fog.rs:303`).
- **Related**: #2488, #3754, #4240, #4261, #3329, NIFAL-D3-2026-09-14-03.
- **Suggested Fix**: Update §2 Particles to match (a)–(d), and mirror the change in the skill's Dimension 5 text.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other load paths)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/emitter.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→canonical boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix

=== #4410 ===
# null: NIFAL-D3-2026-09-14-03: nifal.md "Skinning" prose is stale — #3930 still described as an open proposal, and the cell loader said to read `mesh.skin` "exactly once" [OPEN]

**Source**: `docs/audits/AUDIT_NIFAL_2026-09-14.md` (`/audit-nifal`, HEAD `7374634f5`)

- **Severity**: LOW (doc)
- **Dimension**: Skinning/Lights
- **Tier Violated**: parked-not-leak
- **Game Affected**: Starfield (point 1); all (point 2)
- **Location**: `docs/engine/nifal.md:190-194`, `docs/engine/nifal.md:153-157`
- **Status**: NEW (drift since #3958)
- **Description**:
  1. #3930 is closed and implemented (`SkinAttach` primary), and #4270 now skips the #3549 geometric solve when `SkinAttach` covers every bone. The spec still calls #3930 an open proposal.
  2. The cell loader reads `mesh.skin` four times, two of them positive consumers (proxy bounding sphere at `byroredux/src/cell_loader/spawn.rs:194`; MorphSlot creation at `byroredux/src/cell_loader/spawn/mesh_instance.rs:1010`). The spec says "exactly once, as a negative filter".
- **Evidence**: See Location; `gh issue view 3930` shows CLOSED.
- **Impact**: A future audit would re-propose #3930, and would miss that the cell path already acts on `mesh.skin` positively — which is how NIFAL-D3-2026-09-14-02 went unnoticed.
- **Related**: #3930, #4270, #3958, #2440, NIFAL-D3-2026-09-14-02.
- **Suggested Fix**: Rewrite both passages to the live state.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other load paths)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/emitter.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→canonical boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix

=== #4411 ===
# null: NIFAL-D9-2026-09-14-02: `every_source_derived_material_field_is_pinned_by_a_test` counts comment prose as a pin — its own rationale comment self-pins `alpha` and `alpha_threshold` (shared text-scan weakness in the Lights/Collision resolve scans) [OPEN]

**Source**: `docs/audits/AUDIT_NIFAL_2026-09-14.md` (`/audit-nifal`, HEAD `7374634f5`)

- **Severity**: LOW (test guard; no live masked regression today)
- **Dimension**: Completeness
- **Tier Violated**: harness-coverage gap
- **Game Affected**: all
- **Location**: `byroredux/src/material_translate.rs:2773-2783` (the `pinned` closure; its own comment at `:2776-2777`). Latent siblings: `crates/nif/src/import/walk/lights.rs:276-288` and `crates/nif/src/import/collision/mod.rs:669-685`, whose resolve-side scans cover whole source files, test modules and comments included.
- **Status**: NEW
- **Description**: The guard treats a field as pinned when any `;`-delimited chunk of the test half contains `material.<field>` at a word boundary plus the substring `"assert"` or `".expect("`. Chunks are not comment-stripped, and `"assert"` also matches English prose. The guard's own rationale comment — "Word boundary, so `material.alpha` is not satisfied by an assertion on `material.alpha_threshold`" — contains both needles, so those two fields are pinned by prose alone. The Lights and Collision resolve-side scans match `downcast_ref::<X>` anywhere in the file, so a future comment or test line naming an arm would mark it resolved even if the production arm were deleted.
- **Evidence**: The agent ran a verbatim port of the scanner on a scratch copy with the only two real assertions on `material.alpha` / `material.alpha_threshold` deleted; it still reported both as pinned. With `//` comments stripped, each field drops from 2 matching chunks to 1. The exterior-spawner guard, replayed comment-stripped, still holds on code text for all 6 files. The Lights and Collision scans have no false match today.
- **Impact**: The #3462 contract ("the next added copy cannot slip through") is weaker than stated. Together with NIFAL-D5-2026-09-14-02 and NIFAL-D7-2026-09-14-04, every text-scan or kitchen-sink completeness guard added since #3462 has at least one hole of this shape.
- **Related**: #3462, #4302, NIFAL-D5-2026-09-14-02, NIFAL-D7-2026-09-14-04. These three can reasonably be published as one guard-hardening issue.
- **Suggested Fix**: Strip `//`/`///` comments and string literals before matching, or require the needle inside an `assert…!(` / `.expect(` expression on the same statement. Reword the rationale comment so it cannot self-match. Scan only the production prefix (`split_once("#[cfg(test)]").0`) in `resolved_light_structs` / `resolved_shape_structs`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other load paths)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/emitter.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→canonical boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix

=== #4413 ===
# null: EXAL ground cover §12.12 Phase C: authored-model tier (GRAS models weighted by climate) [OPEN]

Tracker for design `docs/engine/exal-groundcover.md` §12.12 **Phase C — the authored-model tier**: instance each `GRAS` record's own model at density-field points, weighted by climate, honouring its water rule.

Split out so the parked helpers have a live issue to cite (#4383). Umbrella: #3807. Phase 3 (LOD chain) is #4056.

## Context
- Since `f8a900b3` (2026-09-13) blade species are the engine's own, climate-selected; `GRAS` records no longer reach the blade palette. A four-game census (184 records) found every vanilla `GRAS` model is a card clump or opaque mesh, and about half are not grass at all.
- Two helpers in `byroredux/src/groundcover_translate.rs` wait on this phase and are `#[allow(dead_code)]` until it lands:
  - `classify_species_name` — climate from a `GRAS` editor ID (tokens measured over all 168 vanilla records, 2026-09-06).
  - `climate_weights_for` — selection weights per climate, generalising `GroundCoverSpecies::DEFAULT_ARID`'s profile.

## Scope
1. Resolve each worldspace's `GRAS` records to their models through the normal mesh path.
2. Weight record selection by climate via the two helpers above.
3. Honour each record's water rule (above/below/either).
4. Draw instances at density-field points alongside the blade tier; Phase D (the blade ↔ authored-model handoff) stays separate.
=== #4414 ===
# null: NPC combat: ambient faction hostility from FactionRelations [OPEN]

Tracker for the consumer `FactionRelations` is missing (#4373).

## Context
- `Faction.SetEnemy` lowers to `Effect::SetEnemy` and records an undirected hostile pair in `FactionRelations` (`crates/scripting/src/combat.rs`, landed with the MQ101 combat slice in `f61ea044`).
- Nothing reads it: `FactionRelations::is_enemy` has no production caller. Combat is armed only by an explicit script `Actor.StartCombat`, which inserts `AiCombatState` and drives `npc_combat_ai_system` (`byroredux/src/systems/combat_ai.rs`). Every MQ101 `SetEnemy` call is paired with such a `StartCombat` (`Fragment_112` / `Fragment_113` / `Fragment_296`), so the slice works without it.

## Scope
1. Decide when two actors whose factions are hostile start fighting without a script: detection/perception range, line of sight, and the CTDA / combat-style inputs vanilla consults.
2. Resolve an actor's factions through the existing faction membership data, then arm `AiCombatState` from `FactionRelations::is_enemy`.
3. Model the directed neutral flags `SetEnemy` currently declines (`abSelfIsNeutralToOther`, `abOtherIsNeutralToSelf`, #4318), which an undirected pair set cannot represent.
4. Persistence: `FactionRelations` holds plain `u32` FormIDs and is not saved; stable FormID keys are needed before it can round-trip.
=== #4415 ===
# null: Magic runtime: SPLO spell lists → SPEL/ENCH → MGEF application [OPEN]

Tracker for the consumer the parsed magic records are missing (#4376).

## Context
- `EsmIndex` (`crates/plugin/src/esm/records/index.rs`) parses `spells` (SPEL), `enchantments` (ENCH), `magic_effects` (MGEF), `leveled_spells` (LVSP) and Oblivion's `magic_effects_by_code` index on every ESM load. None of them has a production reader.
- No `SPLO` decoder exists, so an NPC's or race's spell list is never resolved. MQ101 `Fragment_11`'s `AddRaceSpells()` declines for exactly that reason (see the `lowers_*` tests in `crates/scripting/src/translate/effects.rs`), which keeps that fragment out of fragment coverage.

## Scope
1. Decode `SPLO` on `NPC_` / `CREA` / `RACE`, resolving leveled spell lists through `leveled_spells`.
2. A canonical spell-list component on spawned actors (translate at the parse boundary, not per game in the runtime).
3. MGEF application: the effect archetypes actor values need first (value modifiers), applied through the existing `ActorValues` modifier composition.
4. Scripting: lower `AddRaceSpells` / `AddSpell` onto (1)–(2), which unblocks MQ101 `Fragment_11`.

Parsers stay as they are; this tracker is about consumers.
=== #4416 ===
# null: IMGS image-space records: DNAM decode + per-cell render consumer [OPEN]

Tracker for the consumer the parsed `IMGS` records are missing (#4214).

## Context
- `parse_imgs` (`crates/plugin/src/esm/records/misc/world.rs`) captures only `EDID` + the raw `DNAM` bytes into `EsmIndex::image_spaces`. Nothing reads the map, so per-cell / per-region HDR, tint and tonemap authoring has no effect.
- Its comment said the decode was "deferred to M48". M48 is the Scaleform UI route and did not touch this.
- Not the same gap as image-space *modifiers*: `IMAD` records already reach the renderer through the MQ101 cinematic slice (`install_image_space_modifiers` → `image_space_modifier_system` → `CinematicPresentationState::image_space_modifier_frame`, consumed in `byroredux/src/app_frame.rs`). The base `IMGS` state those modifiers apply on top of is what is missing.

## Scope
1. Decode `DNAM` per game (HDR eye-adapt / bloom / sunlight scale / cinematic saturation, brightness, contrast, tint) at the parser boundary into one canonical image-space struct.
2. Resolve a cell's `XCIM` (and the worldspace / weather fallbacks) to that struct at cell load.
3. Feed it to presentation/tonemap as the base the existing IMAD modifier frame composes onto — one post-process path, no second tonemap.
=== #4426 ===
# null: NIFAL-D8-2026-09-16-01: Flipbook frames bypass the per-role texture resolver — authored CLAMP is dropped on the vanilla Oblivion gates, and Normal/Height/SmoothSpec flips would upload as sRGB [OPEN]

**Source**: `docs/audits/AUDIT_NIFAL_2026-09-16.md` (texture-roles-deep audit suite, HEAD `7996edf61`)

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

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other games' arms)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs`, per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix

=== #4428 ===
# null: FO4-D2-2026-09-16-02: The BGSM merge binds `envmap_texture` regardless of the chain's `environment_mapping` flag; a bound cubemap switches the shader into "explicit environment" with strength `env_map_scale` (0), wasting RT reflection rays and zer… [OPEN]

**Source**: `docs/audits/AUDIT_FO4_2026-09-16.md` (texture-roles-deep audit suite, HEAD `7996edf61`)

- **Severity**: MEDIUM (performance plus a wrong reflection term, visual only; most affected surfaces are dielectric)
- **Dimension**: 2 (BGSM/BGEM consumption)
- **Location**:
  - `byroredux/src/asset_provider/material/merge.rs:692-698`: unconditional `fill(&mut material.textures.environment, &bgsm.envmap_texture, …)`
  - Its gated siblings: `byroredux/src/asset_provider/material/mod.rs:166-177` (`forward_bgsm_env_map_scale`, gated on `bgsm.base.environment_mapping`) and `merge.rs:1059-1072` (the BGEM env fill, gated on `env_mapping_enabled()` since #2643)
  - Shader: `crates/renderer/shaders/triangle.frag:2865-2870` (`hasAuthoredCubemap = envMapIndex != 0` → `hasExplicitEnvironment` → `needsEnvironmentReflection`), `:2896-2898` (`environmentStrength = max(multiLayerEnvmapStrength, 0)`), `:2976-2977`
  - `byroredux/src/render/static_meshes.rs:858-876` (`multi_layer_envmap_strength = env_map_scale` for non-MLP)
- **Status**: NEW. This is the BGSM sibling of #2643, which gated only the BGEM arm.
- **Description**: The BGSM arm honours the authored `environment_mapping` bit for the mask *scale* but ignores it for the cubemap *texture*. That is the same "one authored bit, honoured for one field and ignored for its neighbour" split that #2643 fixed on BGEM. In `triangle.frag`, a non-zero `envMapIndex` counts as explicit environment authoring:
  - `needsEnvironmentReflection` becomes true even for dielectrics.
  - An RT reflection ray is traced whenever `roughness < 0.6`.
  - The result is scaled by `environmentStrength = env_map_scale`.

  With env mapping disabled, `env_map_scale` keeps the NIF value, which is 0 for every non-Envmap shader type. The traced reflection is therefore multiplied by zero. Conductors (`metalness > 0.3`) that would otherwise take the implicit strength-1.0 path lose their reflection entirely.
- **Evidence**:
  - BGSM corpus: 3,446 BGSMs author `envmap_texture`; **444** of them have `environment_mapping = false` (e.g. `diamondsigns01alphaflatglow.bgsm`, `rrdiabanner01blank.bgsm`).
  - `Fallout4 - Meshes.ba2`, BGSM-backed shapes whose whole template chain has env mapping off but a cubemap is bound anyway: **8,979**. Of these, **6,130** are bound only by this merge fill. The other 2,849 are bound by the NIF's unconditional FO4 slot-4 route (160 NIF-only, 2,689 both).
  - Of the 8,979: 8,806 have `env_map_scale = 0.0`, so the reflection term is multiplied by zero. 7,170 have leaf roughness < 0.6 and pay for a reflection ray. Up to 181 have saturation metalness > 0.3 and lose reflections they get without the cubemap. 173 are `shader_type = 1` with a non-zero scale and gain an env reflection the BGSM disabled.
- **Impact**: Wasted per-fragment RT reflection rays on thousands of vanilla surfaces. Missing reflections on a small set of conductors. Reflections the material turned off appear on 173 shapes.
- **Related**: #2643, #2608, #2472, NIFAL-D8-2026-09-16-03 (same "BGSM enable bit parsed but not consulted" shape for `glowmap`)
- **Suggested Fix**: Gate the BGSM `environment` fill on the chain's `environment_mapping`, as `forward_bgsm_env_map_scale` already does. Separately, decide whether an authoritative BGSM "env mapping off" should also suppress a NIF-sourced slot-4 cubemap. FO4 treats the BGSM as authoritative for material semantics, but the merge's documented rule is "NIF wins". That decision needs a source, as NIFAL-D8-03 argues for `glowmap`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other games' arms)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs`, per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix

=== #4429 ===
# null: SF-2026-09-16-D3-01: The canonical texture-role vocabulary has no role for Starfield's roughness / metalness / AO / opacity / transmissive maps — CDB Phase 2 has nowhere correct to put ~39% of Starfield's textures [OPEN]

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

