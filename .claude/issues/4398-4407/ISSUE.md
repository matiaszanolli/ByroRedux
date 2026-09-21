# 4398: NIFAL-D5-2026-09-14-01: Emitter orientation is dropped, so the authored spawn cone is world-axis aligned and #4240's azimuth wedge points the wrong way on any rotated placement

State: OPEN  Labels: ['bug', 'import-pipeline', 'medium', 'nifal']

**Source**: `docs/audits/AUDIT_NIFAL_2026-09-14.md` (`/audit-nifal`, HEAD `7374634f5`)

- **Severity**: MEDIUM (NIFAL row: translatable particle emitter data silently dropped; no content removed)
- **Dimension**: Particles
- **Tier Violated**: parked-not-leak (authored rotation is parsed, then silently dropped; not recorded as a deferral)
- **Game Affected**: Oblivion, FO3, FNV, Skyrim (Starfield N/A). #4240 measured wedge-authoring emitters at FO3 250/422 and FNV 405/1262.
- **Location**:
  - `crates/nif/src/import/walk/emitter.rs:754-757` (flat walker keeps only `.translation`)
  - `crates/nif/src/import/types.rs:1857-1859` (`ImportedParticleEmitterFlat` has no rotation field)
  - `byroredux/src/cell_loader/spawn.rs:1274-1275` (`GlobalTransform::new(world_pos, Quat::IDENTITY, 1.0)`; the sibling fog branch at `:1225` does receive `ref_rot`)
  - `byroredux/src/systems/particle.rs:410-413` and `:488-511` (reads only `g.translation`; the cone is built around world +Y and world +X)
  - `crates/nif/src/blocks/particle.rs:139` (`NiPSysVolumeEmitter.Emitter Object` ref discarded)
- **Status**: NEW (the rotation half of #1333, which fixed translation only; the #4240 commit message acknowledges the gap, and no issue tracks it — confirmed by `gh issue list --search` on "emitter rotation" / "emitter orientation")
- **Description**: Gamebryo's emitter direction is expressed in the emitter's own frame. No orientation reaches the canonical `ParticleEmitter`:
  - The flat import drops the composed NIF rotation.
  - The cell spawn drops the REFR rotation.
  - `particle_system` ignores any rotation on the entity's `GlobalTransform`, so even the loose-NIF path's carefully set `local_rotation` is unused.

  Before #4240 the azimuth was a uniform random draw, so yaw was invisible. With the authored wedge now forwarded, every authored fan aims relative to world +X regardless of how the placement is yawed.
- **Evidence**: See Location. `authored_planar_angle_aims_the_spawn_azimuth` uses an identity host rotation, so nothing pins rotated hosts. The number of placed wedge emitters on non-identity REFR yaw was not measured.
- **Impact**: Directional FX (sparks, steam vents, spray/impact fans, directional dust) spawn toward a fixed world direction instead of the placed object's facing. The error varies per placement, so in-world it looks random.
- **Related**: #1333, #4240, #984 (force-field directions share the gap).
- **Suggested Fix**: Carry the composed NIF rotation on `ImportedParticleEmitterFlat`. Insert `ref_rot × nif_rot` on the billboard emitter transform at `byroredux/src/cell_loader/spawn.rs:1274-1275`. In `particle_system`, rotate the sampled offset and `dir` by `g.rotation`. Pin with a rotated-host variant of the azimuth test. Separately verify whether force-field directions are emitter-local before rotating them.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other load paths)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/emitter.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→canonical boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix


---

# 4399: NIFAL-D3-2026-09-14-02: GPU morph-target deformation (#3231) creates its MorphSlot only on entities that can never have `bone_offset != 0`, so it is unreachable end-to-end

State: OPEN  Labels: ['bug', 'animation', 'renderer', 'medium', 'nifal']

**Source**: `docs/audits/AUDIT_NIFAL_2026-09-14.md` (`/audit-nifal`, HEAD `7374634f5`)

- **Severity**: MEDIUM (feature inert; per-entity GPU weight/delta buffers allocated and refreshed for nothing; no crash)
- **Dimension**: Skinning/Lights
- **Tier Violated**: no-leak (the cell loader uses raw-tier `ImportedMesh.skin.is_some()` as a stand-in for "has a canonical `SkinnedMesh`", which nifal.md documents as never true on that path)
- **Game Affected**: all games with morph-target content on skinned shapes
- **Location**: `byroredux/src/cell_loader/spawn/mesh_instance.rs:1001-1024` (creation), `crates/renderer/src/vulkan/context/build_and_upload_instances.rs:382-384` (read gate)
- **Status**: NEW
- **Description**:
  - **Where slots are created**: the only production `create_morph_slot_for_mesh` call is in `spawn_mesh_instance` (cell loader only), gated on `mesh.skin.is_some()`.
  - **Where slots are read**: only when `bone_offset != 0`, which requires a `SkinnedMesh`. Per #2440, the only production `SkinnedMesh::new_with_global` is `byroredux/src/scene/nif_loader.rs`.
  - **The other path**: the loose-NIF / NPC path produces `bone_offset != 0` but never creates a MorphSlot.

  The two conditions therefore never meet on any entity.
- **Evidence**: Grep results cited above. #3231's verification was a no-regression live boot, not proof of deformation. The only guard, `morph_spawn_uses_mesh_handle_shared_delta_cache`, pins that the call exists, not that it is reachable.
- **Impact**: `AnimatedMorphWeights` are staged into slots no draw consumes, so authored morph animation never deforms anything. The #4294 LRU-recreation and #3661 residency work maintain buffers that are dead on arrival.
- **Related**: #3231, #2440, #4294, #3661, #2221.
- **Suggested Fix**: Gate slot creation on the canonical signal (the entity will carry a `SkinnedMesh`) and add the equivalent creation on the loose-NIF / NPC path where `SkinnedMesh` is built. Add a reachability test. If loose-NIF wiring is out of scope, stop creating cell-path slots and record the gap beside #2440 in nifal.md.
### LOW

## Completeness Checks
- [ ] **DROP**: If MorphSlot / GPU buffer ownership changes, the Drop / LRU release path stays reverse-order correct
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other load paths)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/emitter.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→canonical boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix


---

# 4400: NIFAL-D1-2026-09-14-03: MSWP swap re-merge (#4290) leaves the BGEM glass-overlay texture roles on the source sidecar while provenance labels follow the target

State: OPEN  Labels: ['bug', 'renderer', 'low', 'nifal']

**Source**: `docs/audits/AUDIT_NIFAL_2026-09-14.md` (`/audit-nifal`, HEAD `7374634f5`)

- **Severity**: LOW (latent: only BGEM v21+ authors these roles; FO4 ships none)
- **Dimension**: Material
- **Tier Violated**: single-boundary
- **Game Affected**: FO76/Starfield-era BGEM through a REFR material swap; latent on FO4
- **Location**: `byroredux/src/cell_loader/spawn/mesh_instance.rs:189-192` (`textures` seeded from `mesh.material`), `:193-202` (`sources` seeded from the swapped `material`)
- **Status**: NEW (introduced by `82c4450d6`)
- **Description**: #4290 moved every re-resolved role and scalar read onto the swapped material, but the `textures` seed still reads the pre-swap cached material. `glass_roughness_scratch` and `glass_dirt_overlay` never pass through `resolve_effective`, so they keep the source sidecar's maps while `MaterialTextureDebugInfo.sources` reports the target's provenance.
- **Evidence**: The orchestrator re-read `byroredux/src/cell_loader/spawn/mesh_instance.rs:187-202` and confirmed `mesh.material.textures.map_ref` next to `material.textures.zip_map_ref`. The #4290 test asserts only scalars and flags.
- **Impact**: Wrong glass scratch/dirt overlay and mislabelled `mat.dump` provenance on swapped BGEM glass. No current FO4 population.
- **Related**: #4290, #973, #3906.
- **Suggested Fix**: Seed `textures` from `material.textures` at `byroredux/src/cell_loader/spawn/mesh_instance.rs:189`. Extend the #4290 test with a BGEM pair that differ in `glass_roughness_scratch`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other load paths)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/emitter.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→canonical boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix


---

# 4401: NIFAL-D8-2026-09-14-02: #4235 moves base-texture precedence to the shader but leaves the paired clamp mode and the parallax slot first-writer-wins

State: OPEN  Labels: ['bug', 'import-pipeline', 'low', 'game:fnv', 'game:fo3', 'nifal']

**Source**: `docs/audits/AUDIT_NIFAL_2026-09-14.md` (`/audit-nifal`, HEAD `7374634f5`)

- **Severity**: LOW (vanilla renders unchanged; mod content exposed)
- **Dimension**: Shader-flags/Effects
- **Tier Violated**: single-boundary (per-role precedence split across two independent latches in one walker)
- **Game Affected**: FO3 / FNV
- **Location**: `crates/nif/src/import/material/legacy_properties.rs:58-70` (`claim_shader_texture`), `:432-437` (clamp latch), `:518` (parallax slot `is_none()`-gated); consumer `byroredux/src/cell_loader/spawn/mesh_instance.rs:933-938`
- **Status**: NEW (introduced by `8dfa78eee`, the fix for closed #4235)
- **Description**: After #4235 the base path always comes from `BSShaderTextureSet`, but `texture_clamp_mode` is still latched by whichever property ran first. When `NiTexturingProperty` is listed first, the shader's texture is sampled with the legacy property's address mode. The parallax/height role has the same split, and there is no in-code deferral comment for either.
- **Evidence**: `claim_shader_texture` clears only the `texturing_property_roles` bits. The clamp write at `:584` stays behind `!texture_clamp_mode_consumed`. The new precedence test asserts paths only.
- **Impact**: On mod content with differing paths: wrong edge wrap/clamp on the shader's texture, and normal/height pairs from different sources. The 5+5 co-bound vanilla shapes name identical paths.
- **Related**: #4235, #3517, #2328, #208.
- **Suggested Fix**: When `claim_shader_texture` displaces a base path, let that block re-latch `texture_clamp_mode`. Apply the same rule to parallax, or add a `#4235` deferral comment at `:518`. Extend the precedence test to assert clamp mode.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other load paths)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/emitter.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→canonical boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix


---

# 4402: NIFAL-D8-2026-09-14-03: #4286 inverted #2108's "a BGSM that wins the greyscale slot is authoritative, including OFF" rule, but the contract comment and test doc still state it

State: OPEN  Labels: ['bug', 'import-pipeline', 'low', 'game:fo4', 'nifal']

**Source**: `docs/audits/AUDIT_NIFAL_2026-09-14.md` (`/audit-nifal`, HEAD `7374634f5`)

- **Severity**: LOW (tiny FO4 population: 11 of 30,166 lit properties have slot 3 empty)
- **Dimension**: Shader-flags/Effects
- **Tier Violated**: no-fabrication (precedence policy changed with no source for which side the engine honours)
- **Game Affected**: FO4 (FO76/Starfield via the CRC-array half)
- **Location**: `byroredux/src/asset_provider/material/merge.rs:656-657` (contract bullet), `:668-684` (code now ORs), `byroredux/src/asset_provider/tests/bgsm_merge.rs:2187-2219` (doc)
- **Status**: NEW (introduced by `d28722fbf`, the fix for closed #4286)
- **Description**: All three merge branches now leave a NIF-set palette bit on, so a BGSM that wins the slot and authors the remap OFF no longer turns it off. The bullet directly above the code still says "(assignment, unchanged)". #4286's justification cited Skyrim-layout BGSM, which barely applies (BGSM is FO4+). The reachable FO4 case was not analysed. Both halves of the #3897/#3898 two-gate invariant still hold; nothing is dropped.
- **Evidence**: `byroredux/src/asset_provider/material/merge.rs:683` `bgsm_greyscale_lut_enabled |= …` sits under the bullet that says "assignment". The old `bgsm_winning_the_slot_still_authors_the_enable_bit_off` test still passes only because its NIF bit is false.
- **Impact**: FO4 content whose BGSM deliberately disables the remap while the NIF enables it renders the palette branch anyway. The larger cost is a self-contradicting precedence contract in the file that owns it.
- **Related**: #2108, #3897, #3898, #4286.
- **Suggested Fix**: Decide the rule from a source (does an FO4 named material file replace the NIF's SLSF1 bits?). Then either restore assignment in the `is_none()` branch or keep OR, and update the `:656-657` bullet and the `byroredux/src/asset_provider/tests/bgsm_merge.rs:2187` doc to match.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other load paths)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/emitter.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→canonical boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix


---

# 4403: NIFAL-D8-2026-09-14-04: Engine docs still describe the pre-#3901 flipbook contract (`texture_slot: u32`, renderer bind "deferred")

State: OPEN  Labels: ['documentation', 'low', 'nifal', 'doc-rot']

**Source**: `docs/audits/AUDIT_NIFAL_2026-09-14.md` (`/audit-nifal`, HEAD `7374634f5`)

- **Severity**: LOW (doc)
- **Dimension**: Shader-flags/Effects
- **Tier Violated**: — (doc describes the removed raw-slot leak as the live shape)
- **Game Affected**: all (Oblivion/FO3/FNV flipbook content)
- **Location**: `docs/engine/animation.md:121-125`, `docs/engine/nif-parser.md:904-906`
- **Status**: NEW (same class as OPEN #4360, which does not list these lines — fold in)
- **Description**: The core `TextureFlipChannel` now carries `role: FlipTextureRole` (`crates/core/src/animation/types.rs:218`). `docs/engine/animation.md` still shows `texture_slot: u32 // raw TexType enum`. `docs/engine/nif-parser.md` still calls the renderer bind deferred, which has been false since #2221 (base role) and #3901 (all roles).
- **Evidence**: grep hits at the cited lines; `docs/engine/animation.md` was last touched before #3901.
- **Impact**: A reader following the doc would reintroduce a raw slot on the canonical channel — exactly the leak #3901 closed.
- **Related**: #3901, #4360, #2221.
- **Suggested Fix**: Update both docs to the shipped `FlipTextureRole` contract; fold into #4360.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other load paths)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/emitter.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→canonical boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix


---

# 4404: NIFAL-D5-2026-09-14-02: The #4167 Particles completeness guards have holes — the structural scan matches comments and substrings, and the value test's `src_blend` equals the preset's

State: OPEN  Labels: ['bug', 'low', 'nifal', 'test-gap']

**Source**: `docs/audits/AUDIT_NIFAL_2026-09-14.md` (`/audit-nifal`, HEAD `7374634f5`)

- **Severity**: LOW (test harness; no runtime impact today)
- **Dimension**: Particles
- **Tier Violated**: single-boundary (the boundary's completeness guard can pass while an overlay is dropped)
- **Game Affected**: all
- **Location**: `byroredux/src/systems/particle.rs:806-847` (structural guard, `body.contains(name)`), `:739` (`Some(6)` passed as `src_blend`), `:773` (assert); `crates/core/src/ecs/components/particle.rs:435` (`torch_flame().src_blend: 6`)
- **Status**: NEW (related #4167, closed by `f680df2e0`)
- **Description**: The structural guard checks raw body text with `contains`, comments included and with no identifier boundary. Most parameter names also appear as substrings of preset fields (`effect_shader` ⊂ `effect_shader_flags`, `greyscale_lut` ⊂ `greyscale_lut_index`), of helper names, or in comments. Separately, the value test passes `src_blend = Some(6)`, identical to the preset, so deleting only the `src_blend` overlay passes both guards.
- **Evidence**: The orchestrator confirmed `byroredux/src/systems/particle.rs:739` passes `Some(6)` inside `every_overlay_parameter_reaches_the_preset` and that `torch_flame()` has `src_blend: 6`. The agent replayed the guard's scan on four mutated copies of the file; the guard passes on all four, including (D) a new parameter mentioned only in a body comment — the exact case the guard exists to catch.
- **Impact**: False confidence; `src_blend` has no working pin.
- **Related**: #4167, #2300, #1513, NIFAL-D7-2026-09-14-04 (same vacuity class).
- **Suggested Fix**: Strip `//` comments before scanning and match whole identifiers not preceded by `.`. Change the fixture's `src_blend` to a value `torch_flame()` doesn't use, and `assert_ne!` every pinned field against `before`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other load paths)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/emitter.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→canonical boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix


---

# 4405: NIFAL-D7-2026-09-14-04: the #4167 animation completeness harnesses have value choices that let specific field drops pass

State: OPEN  Labels: ['bug', 'animation', 'low', 'nifal', 'test-gap']

**Source**: `docs/audits/AUDIT_NIFAL_2026-09-14.md` (`/audit-nifal`, HEAD `7374634f5`)

- **Severity**: LOW (test coverage gap; both boundaries currently correct)
- **Dimension**: Animation
- **Tier Violated**: harness-coverage gap
- **Game Affected**: all
- **Location**: `byroredux/src/asset_provider/animation.rs:425-430` and `:498-499`; `byroredux/src/anim_convert.rs:1070-1095`, `:1079`, `:1113`, `:1272-1282`
- **Status**: NEW
- **Description**: Every field of core `AnimationClip` and its channel/key types is set, so the harnesses are not vacuous overall, but four value choices are not distinctive:
  1. The HKX scale fixture `[1,2,3]` averages to `2 == scale[1]`, so dropping the average passes.
  2. `translation_type: Linear` is the value a hard-coded converter would produce.
  3. `FloatTarget::Alpha` is the first variant, so a hard-coded target passes.
  4. Every channel has one key and only collection counts are asserted, so a first-key-only copy passes.
- **Evidence**: Arithmetic and assertions at the cited lines.
- **Impact**: A regression in exactly the transforms the harnesses were written to guard would ship green.
- **Related**: #4167, #3462, NIFAL-D5-2026-09-14-02.
- **Suggested Fix**: HKX scale `[1,2,6]`; non-Linear key types; a non-first `FloatTarget`; two keys per channel with `keys.len()` asserted.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other load paths)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/emitter.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→canonical boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix


---

# 4406: NIFAL-D7-2026-09-14-03: #4166's `normalized_rotation_sample` substitutes an identity key for the overflow case instead of skipping it, contradicting its own doc and its three siblings

State: OPEN  Labels: ['bug', 'animation', 'low', 'nifal']

**Source**: `docs/audits/AUDIT_NIFAL_2026-09-14.md` (`/audit-nifal`, HEAD `7374634f5`)

- **Severity**: LOW (malformed content only; finite output)
- **Dimension**: Animation
- **Tier Violated**: no-fabrication
- **Game Affected**: FO3/FNV, Skyrim+ (`NiBSplineCompTransformInterpolator`)
- **Location**: `crates/nif/src/anim/bspline.rs:259-264` (doc), `:297-318` (fn), `:430-438` (push); pinned by `crates/nif/src/anim/tests/bspline.rs:191-197`
- **Status**: NEW (introduced by `1cedb6f8e`)
- **Description**: The doc says a bad sample is skipped "so the bone falls back to its bind pose". Only the non-finite input returns `None`; the `len_sq == inf` overflow case is routed into the identity arm and pushed as a real `RotationKey`. A local identity rotation is an invented pose, not the bind pose. The translation, scale and float siblings all skip the sample.
- **Evidence**: The test `bspline_rotation_sample_substitutes_identity_when_squaring_overflows` pins `[1,0,0,0]`.
- **Impact**: On malformed content the bone snaps to identity for the affected span.
- **Related**: #4166, NIFAL-D7-2026-09-14-01 (the shared sanitizer proposed there should return `None` here too).
- **Suggested Fix**: Return `None` when `!len_sq.is_finite()` and update the two pinning tests, or rewrite the doc if identity is deliberate.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other load paths)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/emitter.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→canonical boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix


---

# 4407: NIFAL-D6-2026-09-14-01: `BhkPlaneShape → None` is justified by a trimesh fallback that never fires for its only vanilla instance

State: OPEN  Labels: ['bug', 'low', 'game:skyrim', 'nifal', 'physics']

**Source**: `docs/audits/AUDIT_NIFAL_2026-09-14.md` (`/audit-nifal`, HEAD `7374634f5`)

- **Severity**: LOW (one small underwater egg-cluster file; the deliberate `None` stays sound — only the documented safety net is false). The orchestrator considered the MEDIUM "silently dropped collision shape" row and rejected it: the drop is deliberate and documented at its arm, not silent.
- **Dimension**: Collision
- **Tier Violated**: parked-not-leak (the parked `None` is described as covered downstream, but nothing covers it)
- **Game Affected**: Skyrim SE
- **Location**: `crates/nif/src/import/collision/shape.rs:93-104` (claim), `byroredux/src/cell_loader/spawn/mesh_instance.rs:1318-1325` (gate that rejects the fallback)
- **Status**: NEW (related closed #1334, #4163)
- **Description**: The arm's comment says the dropped plane falls back to "the synthesized-trimesh fallback (spawn.rs) — its render-mesh surface". The one vanilla file, `slaughterfisheggcluster01_1.nif` (`Skyrim - Meshes1.bsa`), has the plane as its only collision and one `BSTriShape` with `NiAlphaProperty` flags `0x12EC`, so `alpha_test = true`. The trimesh fallback requires `!source_material.alpha_test`, so the placement gets no collider. #4163's `plane_shapes` counter has no production reader: it is not in `collision_authoring_totals` or `SpawnCensusAuthoring`, so the drop is invisible at runtime.
- **Evidence**: The orchestrator re-read `crates/nif/src/import/collision/shape.rs:93-104` and `byroredux/src/cell_loader/spawn/mesh_instance.rs:1318-1325`. The agent's `trace_block` probe of the file shows the block list above.
- **Impact**: One cosmetic physics-only collider missing; the documented-limitation rule rests on an unmeasured claim.
- **Related**: #1334, #4163, #2355.
- **Suggested Fix**: Correct the comment, `.claude/commands/audit-nifal/SKILL.md` and nifal.md wording to "no collider is produced for this instance". Optionally fold `plane_shapes` into `collision_authoring_totals` / `SpawnCensusAuthoring`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other load paths)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/emitter.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→canonical boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix


---
