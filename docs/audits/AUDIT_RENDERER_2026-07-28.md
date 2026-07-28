# ByroRedux Renderer Gap Audit — 2026-07-28

Scope: all 23 `/audit-renderer` dimensions at commit
`9bf4c4931cf42c437f8cf6ed3abde082c6c973aa`, plus a renderer-facing
compatibility pass across the NIFAL, EXAL, animation, material, water, weather,
LOD, and Starfield boundaries. The incremental regression baseline is the
previous full renderer audit at `ca7a4e0e` (2026-07-25). This report also
cross-checks the open issue inventory and the 2026-07-27 NIFAL/runtime audits.

The distinction between three kinds of work is deliberate:

1. **Defects** — implemented behavior is wrong or has regressed.
2. **Compatibility gaps** — source-game data reaches the project but does not
   yet affect the final image correctly.
3. **Future renderer scope** — planned quality/features that are not required
   to preserve already-claimed behavior.

Static inspection used the indexed codebase graph first for symbol, caller,
and data-flow discovery, followed by targeted source reads and repository
history. No Vulkan barrier or render-pass change is proposed from static
reasoning alone.

## Executive Summary

The renderer's low-level Vulkan foundation remains healthy. The audit found no
new acceleration-structure corruption, SSBO-index mismatch, GPU ABI drift,
missing synchronization barrier, resource-lifetime failure, or FSR
presentation-chain regression. The 473 renderer tests and 734 application
tests pass.

The most urgent new defect is nevertheless serious:

- **HIGH — shader source/artifact divergence can remove every caustic.**
  `crates/renderer/shaders/composite.frag` was changed to force
  `causticLum = 0.0`, but its committed SPIR-V was not rebuilt. The executable
  currently embeds the older, working artifact; the next normal shader rebuild
  will remove both glass and water caustics. The repository's shader-artifact
  gate fails at HEAD. This is a regression of the failure class previously
  tracked by closed issue #1447.

The renderer-code pass also proved one still-open shading defect:

- **MEDIUM — RT secondary-hit normals are bind-pose approximations for
  skinned meshes.** The BLAS follows deformed geometry, but reflection,
  refraction, glass Fresnel, and transparent-shadow consumers reconstruct the
  hit normal from undeformed global vertices.

The compatibility pass found that several headline systems are only partially
connected to authored content:

- **Authored CELL/WTHR fog is inert.** Fog values are uploaded, but composite
  explicitly leaves them unconsumed and volumetrics uses hardcoded scattering
  and phase constants.
- **Most non-transform animation channels are runtime-dead.** Production spawn
  paths do not attach the `Animated*` sink components the animation system
  requires. Visibility and UV have renderer read paths but are normally
  unreachable; alpha, material color, shader parameter, morph, and flipbook
  channels also lack complete final-image consumers.
- **Imported directional and spot lights become point lights** through the
  already-filed HIGH issue
  [#2205](https://github.com/matiaszanolli/ByroRedux/issues/2205).
- **Water, weather, imagespace, vegetation, LOD, and Starfield material
  support are materially partial**, even where parsing or canonical data
  structures already exist.
- **FO3 Megaton exterior whiteout remains an unfiled render blocker.** The
  symptom is recorded in `ROADMAP.md`, but its suspected Inf/NaN origin is not
  source-proven. It needs RenderDoc and NaN/Inf shader visualization before a
  fix is designed.

### Active defect count

| Class | HIGH | MEDIUM | LOW |
|---|---:|---:|---:|
| New regression since 2026-07-25 | 1 | 0 | 0 |
| Newly proven, currently unfiled renderer defect | 0 | 1 | 0 |
| Newly consolidated compatibility defects | 0 | 2 | 0 |
| Unfiled runtime blocker needing capture | 1 | 0 | 0 |
| Documentation-contract drift cluster | 0 | 0 | 1 |

The compatibility count above includes authored fog and non-transform
animation. It does not count deliberately deferred features, nor does it
recount already-filed issues.

## What Is Already Solid

The audit should not be read as “the renderer is mostly missing.” The following
foundations were reverified and are good:

- Static and skinned BLAS/TLAS build/refit flags, scratch serialization,
  instance-custom-index plumbing, empty TLAS initialization, and deferred
  destruction.
- Camera-relative raster space and absolute RT space remain separated
  consistently, including large-world reconstruction and motion vectors.
- `GpuInstance`, `GpuCamera`, and the live 348-byte `GpuMaterial` layout match
  their shader mirrors and are test-pinned.
- The material table's byte-exact dedup, overflow handling, and canonical
  NIFAL translation boundary remain intact.
- SVGF/TAA history slots, mesh-ID disocclusion, failure latches, and FSR
  switch rollback are structurally coherent.
- The eight-color-attachment G-buffer, presentation pass, query timers, bloom,
  soft-shadow gates, tangent-space import, and light-animation flag
  translation remain internally consistent.
- The FSR Quality path has no newly proven static defect. Its remaining gaps
  are validation and composition-order scope, described below.

## Detailed Findings

### HIGH

#### REN-2026-07-28-01 — `crates/renderer/shaders/composite.frag` and committed SPIR-V disagree; rebuilding disables all caustics

- **Dimension:** 14, with impact on 8 and 15
- **Status:** NEW regression; same failure class as closed #1447
- **Introduced by:** `0a3e0da5` (`docs: Add runtime telemetry audit for
  2026-07-27`)
- **Evidence:**
  - `crates/renderer/shaders/composite.frag:381` assigns
    `float causticLum = 0.0;`.
  - The prior expression summed `causticRaw` and `waterCausticRaw`, promoted
    each accumulator to float before addition, and divided by
    `CAUSTIC_FIXED_SCALE`.
  - `crates/renderer/src/vulkan/composite.rs:39` embeds
    `crates/renderer/shaders/composite.frag.spv` with `include_bytes!`.
  - `scripts/check-shader-artifacts.sh` reports drift for
    `crates/renderer/shaders/composite.frag.spv` with pinned glslang
    11:16.2.0: committed SHA starts `a95f`, rebuilt SHA starts `a01e`.
- **Impact:** Current binaries happen to retain caustics because they use the
  stale, older SPIR-V. A canonical rebuild makes every glass and water caustic
  contribution zero. Source review and runtime behavior therefore describe
  different renderers.
- **Why tests missed it:** 473 renderer tests and 734 application tests pass.
  The existing SPIR-V reflection test pins structural branch properties, not
  the semantic caustic expression. The artifact gate is the only failing
  automated check.
- **Fix:** Restore the combined fixed-point decode, rebuild the SPIR-V, and add
  a source-semantic regression guard that requires both accumulator reads and
  the fixed-scale divide. Keep the artifact gate mandatory for any shader
  source edit.

#### REN-2026-07-28-BLOCK-01 — FO3 Megaton exterior geometry renders pure white

- **Dimension:** 18, with possible interaction in 2, 8, and 17
- **Status:** KNOWN in `ROADMAP.md`, unfiled, Needs-RenderDoc
- **Evidence:** The recorded reproduction has correct sky output while
  structures become uniformly white. Moving exposure from 0.85 to 0.02 does
  not move the geometry response, which argues against ordinary tone-map
  saturation. The current working hypothesis is non-finite data in the
  exterior directional-shadow/GI path, but that is not yet proven.
- **Impact:** A representative FO3 exterior is not meaningfully renderable;
  this invalidates a blanket FO3-exterior “working” claim.
- **Next step:** Add `isnan`/`isinf` debug visualization around direct,
  indirect, shadow, and GI terms, then capture the first corrupt pass in
  RenderDoc. Do not patch exposure, ACES, or the sun path speculatively.

### MEDIUM

#### REN-2026-07-28-02 — skinned RT hit normals use undeformed vertices

- **Dimensions:** 2, 9, 15, 17
- **Status:** Existing implementation gap, currently unfiled
- **Evidence:**
  - `crates/renderer/shaders/include/ray_hit.glsl:39-56`
    reconstructs a triangle normal from the global bind-pose `vertexData`,
    then applies `GpuInstance.model`.
  - `crates/renderer/shaders/skin_vertices.comp` writes position-only,
    compute-skinned absolute-world vertices to the per-entity BLAS input.
  - `crates/renderer/src/vulkan/acceleration/predicates.rs:68-72` documents
    that skinned TLAS instances use identity because the BLAS is already in
    absolute world space, and explicitly calls the hit normal a bind-pose
    approximation.
  - Consumers include `traceReflection`,
    `traceShadowTransmittance`, and direct calls in
    `crates/renderer/shaders/triangle.frag` for glass interfaces,
    refraction exits, and secondary-hit lighting.
- **Impact:** A ray intersects the correct animated surface position but can
  shade it with a normal from the undeformed pose. Moving limbs, cloth-like
  deformation, and animated glass can produce wrong Fresnel, reflection
  lighting, transmission loss, and refraction direction.
- **Fix direction:** Give the shared RT hit helper access to deformed triangle
  positions or a deformed normal stream keyed by the same skinned instance.
  The chosen representation must preserve the compact 12-byte BLAS position
  input and avoid applying the entity transform twice.

#### REN-COMPAT-2026-07-28-01 — authored fog is uploaded but never rendered

- **Dimensions:** 8, 16, 18
- **Status:** Newly consolidated compatibility defect
- **Evidence:**
  - `byroredux/src/render/mod.rs` resolves and uploads CELL/WTHR fog color,
    near/far distance, and FNV cubic-fog parameters.
  - `crates/renderer/shaders/composite.frag:31-45` explicitly marks
    `fog_color` and `fog_params` reserved and unconsumed.
  - `crates/renderer/src/vulkan/context/post_passes.rs:356-379` uses
    `DEFAULT_SCATTERING_COEF`, `DEFAULT_PHASE_G`, and
    `DEFAULT_VOLUME_FAR`. Authored fog color/density does not drive the
    volumetric medium; `fog_far` is only carried as reach.
- **Impact:** Interior XCLL fog, WTHR haze, FNV cubic falloff, FO4 far tint/max
  fields, and Starfield height-fog semantics do not produce their authored
  image. Existing tests prove CPU propagation, not shader consumption.
- **Fix direction:** Translate the canonical fog model into volumetric
  density, extinction, phase, and tint inputs. Preserve interior behavior and
  authored curve semantics; do not simply resurrect the removed
  exterior-only composite mix.

#### REN-COMPAT-2026-07-28-02 — non-transform animation channels lack a complete sink lifecycle

- **Dimensions:** renderer extraction plus animation/material integration
- **Status:** Newly consolidated compatibility defect
- **Evidence:**
  - `byroredux/src/systems/animation.rs` updates
    `AnimatedVisibility`, `AnimatedAlpha`, animated material colors,
    `AnimatedShaderColor`, `AnimatedShaderFloat`,
    `AnimatedUvTransform`, and `AnimatedMorphWeights` only when the target
    component already exists.
  - Repository-wide production insertion search finds no spawn/import
    attachment for these components; the observed insertions are in unit
    tests.
  - `byroredux/src/render/static_meshes.rs` reads only
    `AnimatedVisibility` and `AnimatedUvTransform`. It does not consume
    animated alpha, material colors, shader values, or morph weights.
  - `TextureFlipChannel` is parsed and stored, but
    `crates/core/src/animation/types.rs` explicitly says renderer integration
    is deferred.
- **Impact:** Fire/lava material motion, visibility controllers, fades,
  animated emissive/diffuse effects, UV scrolling, morph targets, and
  Oblivion/FO3/FNV texture flipbooks can parse successfully yet remain
  visually inert. Light color/intensity controllers are the exception because
  they mutate an already-present `LightSource`.
- **Fix direction:** During clip attachment/import, insert only the sink
  components required by the clip's channel types. Then apply animated
  material values at extraction before `GpuMaterial` interning, implement the
  texture-rebind consumer, and connect morph weights to an actual deformation
  path. Add an end-to-end import → system → draw-material test rather than
  helper-only routing tests.

### LOW

#### REN-DOC-2026-07-28-01 — renderer documentation trails the live GPU contract

- **Status:** NEW documentation cluster; runtime ABI is correct
- **Evidence:**
  - `docs/engine/shader-pipeline.md` still describes `GpuMaterial` as 300
    bytes and stops at offset 296; live size is 348 bytes with twelve
    supplemental role indices at offsets 300–344.
  - It still describes `GpuInstance` offset 108 as padding instead of
    `surface_id`, opaque mesh IDs as instance index + 1 instead of stable
    surface identity, and `GpuCamera.render_origin.w` as reserved instead of
    the FSR history-reset flag.
  - `docs/engine/memory-budget.md` still budgets the material buffers at 300
    bytes each / 9.8 MiB total and the scene SSBO total at approximately
    213 MiB. The live figures are 348 bytes / approximately 11.4 MiB and
    approximately 214.6 MiB.
  - The same document calls material overflow silent, while
    `MaterialTable::intern_by_hash` emits a one-shot warning and exposes
    overflow telemetry through `ctx.scratch`.
- **Impact:** No current runtime corruption; contributors and audits can make
  wrong ABI or budget assumptions.
- **Fix:** Refresh both reference documents from the test-pinned layout and
  constants.

## Existing Open Renderer-Facing Defects

These remain relevant and should not be duplicated:

| Priority | Issue | Renderer impact |
|---|---|---|
| HIGH | [#2205](https://github.com/matiaszanolli/ByroRedux/issues/2205) | Imported directional/spot kind, direction, and cone are discarded; GPU upload emits point lights. |
| MEDIUM | [#2215](https://github.com/matiaszanolli/ByroRedux/issues/2215) | Indirect grouping remains regressed: measured GPU calls remain 23 FNV / 31 Oblivion / 48 FO4. |
| MEDIUM | [#2206](https://github.com/matiaszanolli/ByroRedux/issues/2206) | Cell-loaded billboard mode is dropped. |
| MEDIUM | [#2211](https://github.com/matiaszanolli/ByroRedux/issues/2211) | Embedded clip duration ignores transform-key time ranges. |
| MEDIUM | [#2212](https://github.com/matiaszanolli/ByroRedux/issues/2212) | Synthesized FO4 alpha-test threshold can override authored BGSM cutoff. |
| MEDIUM | [#2108](https://github.com/matiaszanolli/ByroRedux/issues/2108) | Effect palette enable is inferred from LUT presence instead of the authored flag. |
| LOW | [#2152](https://github.com/matiaszanolli/ByroRedux/issues/2152) | ReSTIR first-use/resize history can read uninitialized device-local reservoir memory. |
| LOW | [#2109](https://github.com/matiaszanolli/ByroRedux/issues/2109) | Later BGEM glass-overlay, mask-scale, and emittance fields lack sinks. |
| LOW | [#1981](https://github.com/matiaszanolli/ByroRedux/issues/1981) | Live ragdoll bounds can remain at bind origin, causing raster cull pop. The issue should be narrowed: `WorldBound` no longer removes the draw from TLAS. |
| LOW | [#779](https://github.com/matiaszanolli/ByroRedux/issues/779) | Missing early-fragment optimization wastes RT queries on overdraw. |
| LOW | [#1749](https://github.com/matiaszanolli/ByroRedux/issues/1749) | `VulkanContext::new` remains a large constructor and maintenance risk. |

## Missing or Partial Rendering Capability

The following are not all “bugs.” They are the remaining content-fidelity
surface, grouped by what the player sees.

### Lighting, atmosphere, and post-processing

- NIF directional and spot lights need the canonical/GPU light-kind fix in
  #2205. This is broad cross-game content, not a rare format tail.
- Volumetrics are technically live but content-agnostic: fixed scattering,
  phase, and volume extent dominate instead of authored fog/weather/region
  media.
- IMGS/imagespace data is effectively parser-only for final rendering:
  authored exposure, tint, saturation, DOF, bloom controls, LUTs, and eye
  adaptation do not drive the presentation chain.
- The current sky has a gradient, sun, and four cloud layers, but lacks a
  chance-weighted CLMT weather scheduler, precipitation, lightning, wetness
  response, glare/HDR weather controls, moons, stars, moon phases, and aurora.
- There is no per-cell HDR reflection/irradiance probe or authored IBL path;
  environment response still relies on ambient and RT approximations.

### Water

- Water reflection/refraction and the accumulator path exist, but
  `wave_amplitude` and `wave_frequency` stop at the canonical
  `WaterMaterial`; no renderer shader consumes them for vertex displacement.
- GNAM noise textures, authored sun/specular controls, underwater fog and
  scattering tails, and WRLD NAM3/NAM4 distant-water data are not consumed.
- Secondary water-ray hits use an albedo+emission proxy rather than the full
  hit material/normal/direct-shadow shading tree.
- `docs/feature-matrix.md` should not currently claim authored water vertex
  displacement.

### Materials, animation, decals, and character surfaces

- Supplemental `lighting_map_index`, `flow_map_index`, and
  `wrinkle_map_index` are imported, uploaded, hashed, and mirrored in GLSL but
  deliberately unsampled pending coordinate/actor-control semantics.
- NiFlipController flipbooks have no renderer consumer.
- General projected decals remain pending; Starfield PDCL records are
  telemetry-only/skipped. Mesh decals and depth bias are not substitutes for
  a projected-decal pass.
- Authored cast/receive-shadow material flags do not fully control raster/RT
  shadow behavior.
- Stencil semantics and ordered-node draw-order hints are largely absent.
- Full skin/eye subsurface response, hair/fur BRDF, cloth simulation, and
  behavior-driven deformation remain future work. The current Disney/fake-SSS
  paths are useful approximations, not full parity.

### Vegetation and distant scene

- GRAS procedural ground cover has a design document but no runtime renderer
  consumer.
- SpeedTree `.spt` import produces a billboard placeholder, without
  branch/leaf geometry, wind/bend animation, or tree LOD.
- Skyrim/FO4 object/terrain LOD and Oblivion `_far.nif` exist, so they should
  not be reported wholly absent. The remaining gaps are multi-band 8/16/32
  tiers, `.btr` normal maps, and imported `bs_lod_cutoffs` that are discarded.
- The precise FO3/FNV distant landmark gap is named geometry under
  *meshes/landscape/lod/\<worldspace\>/*; those games do not use the previously
  assumed DistantLOD placement-file scheme.

### Starfield and later-content boundaries

- Starfield `.mat` CDB support is Phase 1: the database is detected and the
  material is routed as PBR, but authored texture/scalar data is not yet
  translated into the canonical material.
- Model-less BFCB geometry remains covered by
  [#1576](https://github.com/matiaszanolli/ByroRedux/issues/1576).
- BSGeometry skin data remains covered by
  [#1827](https://github.com/matiaszanolli/ByroRedux/issues/1827).
- Secondary UVs remain covered by
  [#2099](https://github.com/matiaszanolli/ByroRedux/issues/2099).
- Starfield projected PDCL decals, exterior streaming, and full material
  parity remain unsupported.
- FO76 remains parse-oriented rather than a renderer-parity target today.

### Platform/support contract

The device layer treats ray-query support as optional, while renderer comments
and several production paths treat RT as mandatory. Without RT, water is
skipped and the GPU palette/skin chain has no CPU fallback. The project should
choose and document one contract:

- fail early with a clear minimum-GPU requirement, or
- build and test a real non-RT fallback for water, skinning, lighting, and
  presentation.

The current half-supported state is harder to reason about than either choice.

## 23-Dimension Assessment

| Dimension | Assessment |
|---|---|
| 1 — Acceleration structures | PASS on delta. Known static-BLAS recovery/burst-eviction limitations remain outside newly proven defects. |
| 2 — SSBO/index and ray queries | PASS for index safety; carry #2152/#779 and the skinned hit-normal gap. |
| 3 — GPU struct layout | Runtime PASS; reference documentation is stale. |
| 4 — Synchronization/barriers | Static PASS; shared-depth FSR behavior still needs validation-layer hardware confirmation. |
| 5 — GPU memory/lifecycle | PASS; staging failure cleanup, resize teardown, and reverse destruction are sound. |
| 6 — NIFAL material translation | Core boundary PASS; carry #2108/#2109/#2212 and unsampled supplemental roles. |
| 7 — Material table | PASS; dedup, stable slots, hashing, and overflow telemetry are pinned. |
| 8 — Denoiser/composite | Temporal structure PASS; authored fog is inert and caustic source differs from embedded artifact. |
| 9 — GPU skinning/BLAS refit | Build/refit chain PASS; shared RT hit normals remain undeformed and #1981 affects raster bounds. |
| 10 — render origin/precision | PASS statically; Markarth-scale capture remains required. |
| 11 — Pipeline/render pass/G-buffer | PASS; eight color attachments plus depth remain aligned. |
| 12 — Command recording | Correctness PASS; carry #2215 indirect-grouping performance regression. |
| 13 — TAA | PASS; jitter, disocclusion, history, failure latch, and FSR rollback are coherent. |
| 14 — Caustic splat | FAIL: HIGH GLSL/SPIR-V divergence. |
| 15 — Water | Core path partial; authored wave/displacement and secondary-hit quality remain incomplete. |
| 16 — Volumetrics/bloom | Pipeline PASS; authored medium integration is missing. |
| 17 — Disney PBR/soft shadows | Core gates PASS; skinned RT hit-normal defect affects secondary shading. |
| 18 — Sky/weather/exterior | Partial: basic sun/TOD/cloud path works; fog, weather events, celestial layers, and FO3 whiteout remain. |
| 19 — Tangent space/normal maps | PASS across import variants and shader convention. |
| 20 — Debug/GPU telemetry | PASS; 14 timer brackets / 28 queries and active-bit reset are aligned. |
| 21 — Cornell harness | Static PASS; sun/glass/caustic image validation still needs GPU capture. |
| 22 — Light animation | PASS for canonical flicker/pulse flags; separate light-kind loss remains #2205. |
| 23 — FSR/presentation | Static PASS; FP32 path, forced failure, live switching, and post-upscale UI composition need validation/work. |

## FSR 3.1 Residual Scope

FSR Quality is the default path, so its remaining scope deserves explicit
tracking even though no new static defect was found:

- Transparency remains inside the main render pass rather than a separated
  transparency composition stage.
- Scaleform/Ruffle output and the reticle are composited before upscale,
  exposing them to reconstruction blur/ghosting.
- Only the FP16-capable GPU path has been exercised; the FP32 SDK permutation
  remains untested.
- `BYRO_FSR_FORCE_DISPATCH_FAIL=1`, live preset switching, and rollback should
  be rerun under validation layers after any frame-tail or resize change.

## Prioritized Fix Order

1. **Restore and rebuild the caustic shader immediately.** Add a semantic guard
   and make the artifact gate non-optional.
2. **Capture the FO3 exterior whiteout.** Promote the ROADMAP observation to a
   tracked HIGH issue with RenderDoc and NaN/Inf evidence.
3. **Fix skinned secondary-hit normals and #2205 light kinds.** These are
   foundational correctness losses in geometry and lighting, respectively.
4. **Connect authored fog and the animation sink lifecycle.** Both fixes unlock
   large populations of already-parsed cross-game content.
5. **Close existing medium boundary bugs:** #2206, #2211, #2212, and #2108;
   then restore #2215's indirect grouping.
6. **Build the next fidelity tranche:** water authoring, imagespace,
   weather/precipitation, projected decals, ground cover, SpeedTree, and
   multi-band LOD.
7. **Complete Starfield material/geometry paths** before advertising broader
   Starfield renderer coverage.
8. **Only then prioritize stretch rendering:** full SSS/eyes, hair/fur,
   path-traced reference mode, neural denoising, GPU-driven/virtual geometry,
   and VR.

## Needs RenderDoc / Hardware Validation

The following should not be “fixed” from static inspection:

- FO3 Megaton exterior whiteout, with per-term NaN/Inf visualization.
- Skinned secondary-hit normals on a strongly animated actor beside glass or a
  reflective surface.
- Water reflection/refraction and fire-refraction background preservation.
- FSR shared-depth synchronization across two frames in flight.
- FSR forced-dispatch fallback, live preset switching, and a non-FP16 device.
- Cornell directional-sun, glass, and caustic reference captures.
- SVGF temporal history versus final à-trous output convergence.
- Markarth-scale render-origin crossings with TAA, SVGF, water, caustics, and
  RT enabled.
- ReSTIR zero-clear A/B on frame 0 and the first frame after resize.
- Skinned BLAS quality around the 600-refit rebuild threshold and just-woken
  palette refresh.

## Verification

| Check | Result |
|---|---|
| `cargo test -p byroredux-renderer --lib` | PASS — 473 passed, 0 failed |
| `cargo test -p byroredux --bins` | PASS — 734 passed, 0 failed, 4 ignored |
| `scripts/check-shader-artifacts.sh` | **FAIL** — drift in `crates/renderer/shaders/composite.frag.spv` |
| Open renderer-labelled issue inventory | Refreshed; no duplicate filed |
| Worktree before report | Clean |

No code fix and no GitHub issue publication were performed as part of this
audit. Suggested publication command:

`/audit-publish docs/audits/AUDIT_RENDERER_2026-07-28.md`
