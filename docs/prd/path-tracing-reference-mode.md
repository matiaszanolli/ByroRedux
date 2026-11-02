# Path Tracing Reference Mode — Product Requirements

**Status**: PROPOSED (2026-11-02). Revision 4 accepts reference-lighting's
RLT-EXP-001…006 change requests into the exporter's scope (§12). Revision
3 (2026-11-01): this PRD now
owns the Cornell exporter, the EXR writer and the external-renderer
choice outright, and files the first two as their own Phase 1 issues that
reference-lighting depends on (its review round 2 asked for one owner per
tool). Revision 2 folded in review round 1 (console-command inventory,
missing TLAS exclusions and GI clamps in §3.1, ground-cover verify closed,
per-game scope relinked to the ROADMAP matrix, SHaRC licence section
numbers, GRIS convergence wording, determinism softened, cross-PRD
alignment with M-LIGHT and M54 on the radiance cache, the external
renderer and the exporter). Nothing below is
built as a result of this document yet, and the decisions it needs from
the project owner are collected in §10. Statements
about how an external renderer, paper, library licence or original engine
behaves cite external sources; a claim that could not be verified is marked
**Phase 0: verify** instead of being stated as fact.

**Scope**: M51 promises two renderer modes built on the existing ray-query
renderer, in this order. First, a **progressive reference**: a full path
tracer (camera rays, no rasterized primary visibility, no denoiser) that
converges, frame after frame on a static scene, to the ground truth of the
engine's *own* canonical scene (the materials, emitters, environment and
media that the NIFAL / EXAL translation already produced), writes that
result as a linear HDR image with a manifest, and is itself checked against
an external renderer on a scene both can express. Second, an **interactive
path-traced mode** for cinematics and screenshots that uses spatiotemporal
path resampling (ReSTIR PT), a world-space radiance cache for diffuse
transport and the existing denoiser chain, and whose error is measured
against the progressive reference. Out of scope: replacing the realtime
raster + RT path as the default renderer, gameplay at path-traced cost,
spectral rendering, proprietary NVIDIA SDKs linked into the engine, and
neural caches (§4.2, §10 Q3). The user is the renderer developer first (an
oracle for every realtime change) and the player second (a photo /
cinematic mode).

**Related**: this document states the *promise*; the designs below own the
*how*. Every mechanism promised here has an owner in this table, or Phase 0
writes the missing design.

| Mechanism | Owning design |
|---|---|
| Deterministic captures, convergence frames, denoiser A/B, manifests | [Renderer Evaluation](../engine/renderer-evaluation.md) |
| Canonical emitters, units, visibility layers, shared transport, ray budget | [Physical Lighting Backbone](../engine/physical-lighting-backbone.md) |
| Cornell L0–L5 oracle, debug views, `light.dump` / `mat.dump`, RT CI workflow | [RT Lighting & Material Recovery](../engine/rt-lighting-material-recovery.md) |
| Participating media (froxel fog, combustion field) | [Procedural Volumetric Fog](../engine/procedural-volumetric-fog.md) |
| TLAS / BLAS, skinned BLAS refit, SVGF, TAA, Disney BSDF | [Vulkan Renderer](../engine/renderer.md) |
| Shader files, pass order, GPU data types | [Shader Pipeline](../engine/shader-pipeline.md) |
| Screen-sized buffer budgets | [Memory Budget](../engine/memory-budget.md) |
| Path-tracer kernel, radiance cache, path reservoirs | none yet — Phase 0 writes `docs/engine/path-tracing.md` |
| Realtime lighting gaps measured against an offline reference; the rung content M-LIGHT adds to this PRD's exporter (RLT-REF-001) | [Reference Lighting PRD](reference-lighting.md) (M-LIGHT, consumes this PRD's exporter, EXR writer, reference and radiance cache) |
| Neural spatial denoiser trained and scored against the progressive reference | [Neural Denoiser PRD](neural-denoiser.md) (M54, consumes the reference as its R1 tier) |

Normative terms **MUST**, **SHOULD** and **MAY** have their RFC 2119
meanings, as in the related designs. Requirement IDs use the `PTR-` prefix.

---

## 1. Product thesis

ByroRedux is RT-first ([ROADMAP keep list](../../ROADMAP.md#the-keep-list--what-not-to-change-despite-temptation)).
Its realtime image is the sum of separately estimated, separately biased
terms: ReSTIR-DI direct light, a primary-hit RT reflection, a bounded
one-path-per-pixel GI walk, glass refraction, caustic splats, a froxel
medium, SVGF, TAA and a tone-map chain (§3.1). Each term has its own
truncations and its own denoiser assumptions, and the evaluation harness
can today only compare the renderer with *itself* — earlier revisions,
toggled reuse, toggled à-trous ([renderer-evaluation.md](../engine/renderer-evaluation.md#how-to-evaluate-a-renderer-change)).
No capture says how far any of those images is from the right answer.

M51 is the right answer. The ROADMAP row frames it as "reference mode for
screenshots / cinematics; demonstrates 'RT-first' wasn't a positioning
claim" ([Tier 8](../../ROADMAP.md#tier-8--visual-fidelity-stretch-post-tier-4-horizon)).
This PRD reads that row as two products in a fixed order: the reference
that can judge, then the interactive mode it judges (§10 Q1).

The corollary that shapes every requirement below: **the reference is
ground truth for the engine's canonical model, never a second model.** It
evaluates the same translated materials, emitters and environment the
realtime path consumes, through the same shared shader functions, and
differs only in the estimator — no truncation, no reuse, no filtering. That
makes the difference between the two a measurement of estimator error
alone, and it means the reference itself has to be proven against an
independent renderer before it is trusted (PTR-VAL). The reference does not
make content look more like the original games, does not add per-game
looks, and does not fix translation errors: a wrong canonical material
renders wrongly in both, which is what the material debug views are for.

---

## 2. Terminology

| Term | Meaning |
|---|---|
| **Progressive reference** | The M51 mode that accumulates independent path samples per pixel on a frozen scene, with no temporal reuse and no denoiser, until a stated sample count. Its output is the *reference image*. It is what [neural-denoiser.md](neural-denoiser.md) calls tier **R1** and what [reference-lighting.md](reference-lighting.md) RLT-REF-005 calls M51's "converged Cornell capture". |
| **Interactive PT mode** | The M51 mode that path-traces every pixel each frame with spatiotemporal reuse, a radiance cache and the existing denoiser chain; biased, and measured against the progressive reference. It is the denoised "reference mode at its default sample budget" of NDN-G-006 and the "PT mode" of reference-lighting's T14. Sibling PRDs that say "M51's reference mode" mean the progressive reference when they mean ground truth and this mode when they mean a denoised image. |
| **External renderer** | The independent offline renderer the progressive reference is proven against (PTR-VAL-002). The same tool as reference-lighting's Q1 "converged reference"; one choice serves both PRDs (§10 Q5). |
| **Realtime path** | The default renderer at HEAD: raster primary visibility in `triangle.vert` / `triangle.frag` plus the ray-query terms of §3.1. Unchanged by M51. |
| **Canonical scene** | What the renderer receives after translation: `GpuInstance`, `GpuMaterial`, `GpuLight`, the TLAS, the sky / environment and the media parameters ([shader-pipeline.md](../engine/shader-pipeline.md#gpu-data-types)). The only input either PT mode reads. |
| **spp** | Samples (complete camera paths) per pixel accumulated into a reference image. |
| **Accumulation reset** | Discarding the accumulated samples because the camera, the canonical scene or a mode parameter changed. |
| **Parity scene** | A Cornell variant whose every material and emitter is expressible exactly in the external renderer (§10 Q5), used to prove the progressive reference. |
| **Transport term** | One row of §3.1: a light-transport contribution the realtime path estimates separately. |
| **Disposition** | How the progressive reference treats a content class, one of the three in §3. |
| **relMSE** | Relative mean squared error of an image against a reference image: the mean over pixels and channels of `(x − r)² / (r² + ε)`, with ε recorded in the manifest. |
| **ReSTIR PT** | The path-space resampler of Lin, Kettunen, Bitterli, Pantaleoni, Yuksel and Wyman, "Generalized Resampled Importance Sampling: Foundations of ReSTIR", ACM TOG 41(4), SIGGRAPH 2022, DOI [10.1145/3528223.3530158](https://doi.org/10.1145/3528223.3530158) ([project page](https://graphics.cs.utah.edu/research/projects/gris/)). The ROADMAP M51 row credits it to "Bitterli et al."; Bitterli is the third author. |
| **Hash-grid radiance cache** | A world-space cache of outgoing radiance keyed by a hashed quantized position, the technique of Binder, Fricke and Keller, "Massively Parallel Path Space Filtering" ([arXiv:1902.05942](https://arxiv.org/abs/1902.05942)). NVIDIA ships a cache of this kind as SHaRC, "Spatially Hashed Radiance Cache", as shader-only sources with an integration guide `docs/Integration.md` ([NVIDIA-RTX/SHARC](https://github.com/NVIDIA-RTX/SHARC)); its README cites no paper, so no lineage between the two is claimed here. The ROADMAP row's "SHARC" and [reference-lighting](reference-lighting.md) Q5's "M51's hash-grid radiance cache" both name this component, whatever its provenance under §10 Q2. The ROADMAP row's "Pharr et al. (SHARC, 2024)" has no source this PRD could find (**Phase 0: verify**, V14). |

---

## 3. Core model

Every content class the canonical scene can contain has exactly one
disposition in the progressive reference:

| Class | Name | Meaning | Reported as |
|---|---|---|---|
| **T1** | Traced | Estimated without truncation: every vertex evaluates and samples the canonical BSDF, light is gathered by next-event estimation and by hitting emitters, paths end only by Russian roulette or by escaping. | `traced` |
| **T2** | Shared-model | A canonical term that is not physically based (a legacy attenuation curve, an authored rim / back-lighting term, an effect-shader card) is evaluated exactly as the realtime path evaluates it, at every vertex where it applies. Its non-physicality is the model's, not the estimator's. | `shared-model: <term>` |
| **T3** | Excluded | The class is absent from the reference's paths. The reference image is still written, and its manifest names the class and the share of primary-hit pixels it would have covered (PTR-OUT-004). | `excluded: <class>, <reason>` |

A class moves from T3 to T2 or T1 only by a revision of §3.1 that cites the
requirement that moved it.

### 3.1 Transport inventory

The realtime column is the state at HEAD on 2026-11-01 (files read at HEAD
`66550cf8a`). It is hand-written; Phase 1 replaces it with the
`render.reference status` report (PTR-DX-002), after which the column is
deleted and this table keeps only the disposition.

| # | Transport term or content class | Today (realtime path) | Reference disposition |
|---|---|---|---|
| 1 | Direct light from `GpuLight` emitters | ReSTIR-DI with temporal and spatial reuse, one reservoir per pixel (`crates/renderer/src/vulkan/restir.rs` header, `RESERVOIR_STRIDE = 36`), plus the `shadow_filter.comp` à-trous on the shadow ratio | T1 (PTR-EST-002); attenuation curve T2 (§10 Q6) |
| 2 | Emissive surfaces as light sources | Contribute only when a GI path segment happens to hit them (`rayHitEmission` in the `triangle.frag` GI loop); no emissive-triangle sampling exists in `crates/renderer/src`. The Cornell box adds a point light under its emissive panel because "emissive-only GI is a known weak spot" (`byroredux/src/cornell.rs`, ceiling light block) | T1 (PTR-EST-003) |
| 3 | Diffuse indirect | One path per pixel per frame; `MAX_DIFFUSE_BOUNCES = 2` (`triangle.frag` GI block), `MAX_PATH_SEGMENTS = 6` (`crates/renderer/src/shader_constants_data.rs`), paths cut when throughput falls below 0.02 with no Russian-roulette compensation, every segment's ray capped at a 6 000-unit `tMax`, GI faded out between 4 000 and 6 000 world units from the camera and gated off past `RT_LOD_GI`; direct light at path hits limited to `MAX_SHADED_HITS = 2` hits and `GI_VISIBLE_LIGHT_CAP = 2` lights; each path sample's luminance clamped to `GI_SAMPLE_LUMINANCE_CLAMP = 1.0` (`shader_constants_data.rs`); no GI path starts from glass, window or emissive primaries (`giTransportEligible`, `triangle.frag`); SVGF temporal + à-trous on the albedo-demodulated result | T1 (PTR-EST-001) |
| 4 | Primary glossy / metallic reflection | A separate RT reflection on the direct attachment; the GI primary event samples the diffuse lobe only (comment at the head of the `triangle.frag` GI block) | T1 |
| 5 | Secondary-vertex BSDF | `samplePathBsdf` (`crates/renderer/shaders/include/pbr.glsl`), a GGX / diffuse mixture, not the full primary Disney lobe set | T1 with the primary lobe set (PTR-MAT-002) |
| 6 | Glass | Primary refraction pass; inside GI paths a glass hit refracts and scales throughput by `mix(1, glassTint, 0.25)` | T1 (PTR-MAT-003) |
| 7 | Alpha-tested and alpha-blended surfaces | Ray coverage through `rayHitHasCoverage` (`include/ray_hit.glsl`); blended surfaces are raster-sorted | T1 for coverage; blend **Phase 0: verify** (V7) |
| 8 | Effect-shader surfaces | In the TLAS on the effect visibility layer; GI adds their emission and stops (`MATERIAL_KIND_EFFECT_SHADER` arm of the GI loop) | T2 |
| 9 | Sky / environment | `pathEnvironmentRadiance` on a GI miss (`include/lighting.glsl`): exteriors sample the baked sky cube; DALC cells sample the DALC cube; other interiors return a constant authored ambient (`sceneFlags.yzw / π × 0.5`) | Exterior sky cube T1; interior authored-ambient escape T2 (`shared-model: interior ambient`), the term reference-lighting Q2 decides |
| 10 | Water | Excluded from the TLAS (`!draw_cmd.is_water` in `draw_command_eligible_for_tlas`, `crates/renderer/src/vulkan/acceleration/predicates.rs`); shaded by the separate `water.vert` / `water.frag` pipeline | T3 until PTR-CNT-002 |
| 11 | Particles | Emitted with `in_tlas: false` (`byroredux/src/render/particles.rs`) | T3 until PTR-CNT-003 |
| 12 | Participating media | Froxel inject / integrate only; the fog design lists ratio tracking and a majorant grid for path-traced media as unbuilt ([follow-up 5](../engine/procedural-volumetric-fog.md#follow-up-boundary)) | T3 until PTR-CNT-004 |
| 13 | Skinned actors | Per-entity skinned BLAS refit (`crates/renderer/src/vulkan/acceleration/blas_skinned.rs`) | T1 on a frozen pose (PTR-CNT-001) |
| 14 | Ground cover blades | GPU-scattered blades with "no BLAS, no TLAS entry" (`crates/renderer/shaders/groundcover_blade.frag` header); V8 closed in rev 2 | T3 (camera rays cannot hit them) |
| 15 | Image output | 8-bit sRGB PNG of the swapchain after presentation (`crates/renderer/src/vulkan/context/screenshot.rs`, "Swapchain format is B8G8R8A8_SRGB") | Linear HDR file (PTR-OUT-001) |
| 16 | Decal surfaces (`IsDecalMesh`) | Raster-only: `TlasExclusion::DecalSurface` keeps them out of the TLAS so they do not occlude the surface they are painted on (`byroredux/src/render/static_meshes.rs`) | T3 until a decal treatment is designed (camera rays would show the undecorated surface) |
| 17 | Fire-refraction cards (`MATERIAL_KIND_FIRE_REFRACTION`) | Raster-only: `TlasExclusion::FireRefraction`; also refused by `draw_command_eligible_for_tlas` | T3 |
| 18 | Distant LOD blocks | Enter the TLAS only inside a camera-local shadow-caster reach (`TlasExclusion::DistantLodBlock`); farther blocks are raster-only | T3 beyond that reach; the manifest reports the share |

Two inventory findings bound the ROADMAP row's "reuses existing reservoir
+ denoiser plumbing": the ReSTIR-DI reservoir record stores a packed light
index (`MAX_LIGHTS = 1023` with index 1023 as the invalid sentinel,
`shader_constants_data.rs`), not a path, so only the buffer ownership,
ping-pong and claim-word pattern of `ReservoirBuffers` carries over to path
reservoirs; and the renderer is ray-query only (`RT_EXTENSIONS` in
`crates/renderer/src/vulkan/device.rs` lists acceleration structure, ray
query and deferred host operations; there is no ray-tracing-pipeline
extension), so both PT modes are compute passes issuing ray queries unless
§10 Q4's alternative is taken. A third: the reference reads the same
TLAS the realtime path builds, so every class the realtime path keeps out
of the TLAS (rows 10, 11, 14, 16–18) is T3 in the reference until a
requirement adds a ray representation for it.

---

## 4. Goals and non-goals

### 4.1 Goals

- **PTR-G-001 — A trusted reference.** The progressive reference renders
  the parity scene within the PTR-VAL-002 tolerance of the external
  renderer, and its error decays at the Monte Carlo rate (PTR-VAL-001).
- **PTR-G-002 — The realtime path is measurable.** Every Cornell oracle
  rung and the FNV real-content probe produce a realtime-vs-reference relMSE
  per transport term in their manifests (PTR-VAL-003).
- **PTR-G-003 — Content coverage.** The share of primary-hit pixels whose
  class is T3 is reported per scene and falls phase by phase (§8).
- **PTR-G-004 — An interactive path-traced mode.** The interactive PT mode
  renders the Cornell and FNV benches within the frame budget the owner
  sets in §10 Q7, with its relMSE against the reference published.
- **PTR-G-005 — Screenshots and cinematics.** A user can capture a
  reference still or a fixed-camera-path sequence headless, from the CLI or
  `byro-dbg`, without editing code.

### 4.2 Non-goals

- **PTR-NG-001.** Replacing the realtime path as the default renderer or
  removing it; the keep list keeps the hybrid design
  ([keep list](../../ROADMAP.md#the-keep-list--what-not-to-change-despite-temptation)).
- **PTR-NG-002.** Matching the original games' look; anti-scope says
  render correctly, not per-pixel parity
  ([anti-scope](../../ROADMAP.md#what-we-are-not-doing-anti-scope)).
- **PTR-NG-003.** Neural radiance caching (the ROADMAP row's "optional
  NRC"; Müller, Rousselle, Novák and Keller, "Real-time Neural Radiance
  Caching for Path Tracing", ACM TOG 40(4), SIGGRAPH 2021,
  [arXiv:2106.12372](https://arxiv.org/abs/2106.12372), DOI
  10.1145/3450626.3459812). Deferred to after M54
  ([neural-denoiser.md](neural-denoiser.md)) builds the engine's neural
  inference path, unless §10 Q3 decides otherwise.
- **PTR-NG-004.** Linking a proprietary NVIDIA SDK (SHaRC, NRC library,
  DLSS Ray Reconstruction). SHaRC ships under the "NVIDIA RTX SDKs LICENSE",
  whose §1(b) grants modification of "sample source code" only, whose §4(e)
  says "You may not use the SDK in any manner that would cause it to become
  subject to an open source software license", and whose supplement §1
  requires applications to be "fully interoperable with compatible GPU
  hardware products designed by NVIDIA"
  ([License.md](https://github.com/NVIDIA-RTX/SHARC/blob/main/License.md));
  ByroRedux is MIT (`LICENSE`). See §10 Q2.
- **PTR-NG-005.** Spectral, polarized or bidirectional transport; the
  canonical scene is RGB.
- **PTR-NG-006.** Path tracing on devices without ray query; RT is
  mandatory at HEAD (`device.rs`, #3759).
- **PTR-NG-007.** Gameplay at path-traced cost: neither PT mode is promised
  to hold the realtime frame budget, and simulation is frozen in the
  progressive reference.

---

## 5. Requirements by area

### 5.1 Mode and accumulation (`PTR-MODE`)

- **PTR-MODE-001.** The progressive reference MUST be selectable at launch
  with a CLI flag and at runtime with a `byro-dbg` console command, and
  MUST leave the realtime path unchanged when not selected. Today: no PT
  mode exists. At HEAD on 2026-11-01 the renderer console commands are
  `render.debug`, `r.dbg`, `r.shadowfilter`, `r.upscaler`, `ctx.upscaler`,
  `r.health`, `rt.masks` and `rt.integrity`
  (`byroredux/src/commands/world_info.rs`) plus `exposure`, `tonemap` and
  `light.atten` (`byroredux/src/commands/scene.rs`); none selects a PT
  mode.
- **PTR-MODE-002.** In the progressive reference, primary visibility MUST
  come from camera rays generated by the path tracer, with sub-pixel
  positions drawn from the sampler, so no raster draw contributes to the
  reference image (ROADMAP: "no rasterized fallback"; §10 Q4).
- **PTR-MODE-003.** The progressive reference MUST accumulate samples only
  while the bench mode is `renderer-static`
  ([determinism contract](../engine/renderer-evaluation.md#determinism-contract))
  or the camera and canonical scene are unchanged, and MUST perform an
  accumulation reset on any change of camera, TLAS instance set, material
  table, light set, environment, attenuation setting (`light.atten`) or
  mode parameter. `exposure` and `tonemap` change only the preview
  (PTR-MODE-007) and MUST NOT reset it.
- **PTR-MODE-004.** The current spp and the frames since the last
  accumulation reset MUST be reported by `render.reference status` and in
  the `bench:` line.
- **PTR-MODE-005.** The progressive reference MUST stop accumulating at a
  target spp given on the CLI and MAY keep presenting the result.
- **PTR-MODE-006.** No single queue submission of either PT mode SHOULD
  exceed the per-submit ceiling recorded by V11; spp per frame MUST be
  bounded so that a reference at any target spp completes without device
  loss. Today `AdaptiveRayBudget` already guards against "watchdog-scale
  work" for the realtime path (`crates/renderer/src/vulkan/scene_buffer/ray_budget.rs`).
- **PTR-MODE-007.** The progressive reference MUST NOT run auto-exposure,
  TAA, FSR upscaling, bloom, grading or tone mapping on the image it
  writes; the presented preview MAY tone-map a copy.
- **PTR-MODE-008.** Selecting the interactive PT mode MUST replace every
  transport term of §3.1 with the path tracer's estimate for that frame;
  the realtime direct, reflection, refraction and GI terms MUST NOT be
  summed with it.

### 5.2 Estimator (`PTR-EST`)

- **PTR-EST-001.** The progressive reference MUST be an unbiased estimator
  of the canonical scene's radiance for every T1 class: no fixed bounce
  cap, no throughput cutoff without Russian-roulette reweighting, no
  distance fade, no fixed ray `tMax` below the scene bounds, no per-hit
  light cap. Today the GI walk applies all five, plus the luminance clamp
  PTR-EST-006 forbids (§3.1 row 3).
- **PTR-EST-002.** Each path vertex MUST perform next-event estimation
  towards `GpuLight` emitters with multiple importance sampling against
  BSDF sampling, using the shared material-aware visibility of the
  backbone ([runtime contract item 4](../engine/physical-lighting-backbone.md#runtime-contract)).
- **PTR-EST-003.** Emissive triangles MUST be sampled as light sources
  with MIS, so the Cornell parity scene renders with its emissive panel as
  the only emitter (PTR-VAL-002).
- **PTR-EST-004.** The sampler MUST be seeded from (pixel, sample index,
  vertex index) only, checked by a shader-source test that no other input
  (frame counter, time, camera) reaches the seed. Two runs of the same
  binary, scene and spp on the same device SHOULD produce byte-identical
  linear output; whether acceleration-structure builds are deterministic
  enough to promise it is **Phase 0: verify** (V13).
- **PTR-EST-005.** Non-finite samples MUST be counted and excluded, and the
  count MUST appear in the manifest; a reference with a non-zero count is
  reported as failed, not written as valid. Today `r.health` reports
  pre-tonemap non-finite pixel counters for the realtime image
  (`RenderHealthCommand`, `world_info.rs`, at HEAD on 2026-11-01); the PT
  count is per sample, not per pixel.
- **PTR-EST-006.** The progressive reference MUST NOT clamp sample
  radiance (firefly clamping). The interactive PT mode MAY clamp, and its
  manifest MUST state the clamp. Today the realtime GI walk clamps each
  path sample's luminance to `GI_SAMPLE_LUMINANCE_CLAMP` (§3.1 row 3).

### 5.3 Materials (`PTR-MAT`)

- **PTR-MAT-001.** Both PT modes MUST evaluate materials through the shared
  shader includes the realtime path uses (`include/pbr.glsl`,
  `include/ray_hit.glsl`, `include/material_sampling.glsl`), not a second
  material implementation; new sampling or pdf routines PTR-MAT-002 needs
  are added to those includes. Today those includes serve the realtime
  path at HEAD on 2026-11-01.
- **PTR-MAT-002.** Every vertex of a reference path MUST evaluate the full
  primary-hit lobe set of the canonical material (the Disney diffuse,
  sheen and anisotropic GGX lobes of [renderer.md § Disney BSDF](../engine/renderer.md#disney-bsdf)),
  with a sampling routine whose pdf is evaluable for MIS. Today secondary
  GI vertices use the reduced `samplePathBsdf` mixture (§3.1 row 5). A lobe
  without an evaluable pdf is **Phase 0: verify** (V1) and is T2 until it
  has one.
- **PTR-MAT-003.** Glass MUST be a dielectric interface with Fresnel
  reflection and refraction chosen stochastically, transmission tinted by
  the canonical glass colour; the realtime `mix(1, glassTint, 0.25)`
  throughput scale MUST NOT appear in the reference.
- **PTR-MAT-004.** Canonical terms with no physical counterpart (the
  soft / rim / back-lighting inputs of the 432-byte `GpuMaterial`, the
  `LegacySoftRange` attenuation of `crates/core/src/lighting.rs`) MUST be
  listed by name in the manifest's T2 list whenever a scene uses them.
- **PTR-MAT-005.** The PT modes MUST NOT read the source game or file
  format: per-game material differences stay in the NIFAL translate step
  ([material-abstraction.md](../engine/material-abstraction.md)), and a
  shader-source test MUST fail if a PT shader references a game identifier.

### 5.4 Content coverage (`PTR-CNT`)

- **PTR-CNT-001.** Skinned actors MUST appear in reference paths at the
  pose of the frame on which accumulation started, using the existing
  skinned BLAS. The hardware-gated `cornell_skinned_actor_receives_light_and_casts_shadows`
  scene (`byroredux/tests/cornell_rt_oracle.rs`) is the acceptance scene.
- **PTR-CNT-002.** Water surfaces SHOULD become T1 by a TLAS-visible water
  representation whose BSDF is the canonical water material; until then
  the manifest MUST report water as T3. Today water is excluded from the
  TLAS (§3.1 row 10).
- **PTR-CNT-003.** Particles MAY become T2 (emission-only, non-occluding,
  matching the effect visibility layer); until then they MUST be reported
  as T3.
- **PTR-CNT-004.** Homogeneous and procedural participating media SHOULD
  become T1 by delta or ratio tracking against a majorant, as the fog
  design's follow-up 5 proposes; the combustion field, which exists only in
  the view-aligned froxel grid, is T3 until V6 settles whether it can be
  sampled in world space.
- **PTR-CNT-005.** Every T3 class present in a scene MUST be listed with its
  primary-hit pixel share in the manifest (PTR-OUT-004), so a reference of
  a scene with water or fog never passes silently as complete.

### 5.5 Output (`PTR-OUT`)

- **PTR-OUT-001.** The progressive reference MUST write the accumulated
  radiance as a 32-bit-float RGB OpenEXR file at render resolution, in
  scene-linear units before exposure. Today screenshots are 8-bit sRGB PNG
  of the presented swapchain (§3.1 row 15); the `image` crate is built with
  `png` only (`Cargo.toml` workspace dependency). The `exr` route is
  **Phase 0: verify** (V5). The writer is the project's one EXR writer,
  owned here; reference-lighting's linear captures (RLT-REF-007) use it,
  so it is filed as its own Phase 1 issue that needs nothing else from
  that phase.
- **PTR-OUT-002.** Each written reference MUST carry a manifest with: git
  revision and dirty state, device and driver, scene arguments, camera, spp,
  wall time, the non-finite count (PTR-EST-005), the T2 list (PTR-MAT-004),
  the T3 list (PTR-OUT-004), and the SHA-256 of the EXR, in the shape of
  the existing `manifest.tsv` ([renderer-evaluation.md § Quick start](../engine/renderer-evaluation.md#quick-start)).
- **PTR-OUT-003.** The reference SHOULD also write albedo and shading-normal
  auxiliary images at the first non-specular vertex, for denoiser training
  and evaluation (M54, [neural-denoiser.md](neural-denoiser.md) R1 tier).
- **PTR-OUT-004.** The manifest MUST report, per T3 class, the fraction of
  pixels whose first camera-ray hit would have been that class.
- **PTR-OUT-005.** The `byro-dbg` `screenshot` command MUST keep its
  current behaviour; the reference image MUST be written by a separate
  command so the PNG contract of the golden-frame tests is unchanged.

### 5.6 Validation (`PTR-VAL`)

- **PTR-VAL-001.** On the Cornell L5 rung, the relMSE of the progressive
  reference against itself at a high spp MUST decrease with a log-log slope
  within a tolerance of −1 across at least four doublings of spp; a
  plateau is the bias signal this test exists to catch.
- **PTR-VAL-002.** On the parity scene, the progressive reference MUST
  match the external renderer's image (§10 Q5) within a relMSE tolerance
  fixed in Phase 0, at matched resolution, camera, spp and with both run
  in linear RGB.
- **PTR-VAL-003.** `scripts/renderer-eval.sh` MUST gain a reference case:
  its manifest records the realtime image's relMSE against the progressive
  reference for the final frame, and per transport term using the existing
  `DirectOnly` / `IndirectOnly` debug views against the matching reference
  split, as reference-lighting's component-decomposition contract
  ([§3.4](reference-lighting.md#34-component-decomposition-contract))
  defines it.
- **PTR-VAL-004.** The parity scene MUST be exported to the external
  renderer's scene format by engine code from the same ECS entities that
  `--cornell` spawns, not by a hand-maintained copy; the exported file MUST
  be deterministic so a diff shows any scene drift. This PRD owns the
  project's one exporter; reference-lighting extends its scene content for
  its rungs (RLT-REF-001) and depends on it, so it is filed as its own
  Phase 1 issue that needs nothing else from that phase.
- **PTR-VAL-005.** The interactive PT mode's relMSE against the progressive
  reference MUST be published per bench scene at each phase exit, alongside
  the realtime path's.

### 5.7 Interactive path tracing (`PTR-RT`)

- **PTR-RT-001.** The interactive PT mode MUST resample paths
  spatiotemporally per ReSTIR PT (Lin et al. 2022, §2), with the path
  reservoirs held in screen-sized per-frame-in-flight buffers that follow
  the `ReservoirBuffers` ping-pong and claim pattern
  (`crates/renderer/src/vulkan/restir.rs`).
- **PTR-RT-002.** The interactive PT mode MUST terminate diffuse paths into
  a hash-grid radiance cache (§2) updated from a sparse subset of paths each
  frame; the cache's implementation provenance follows §10 Q2. The cache's
  query and update entry points SHOULD live in a shared shader include so
  the realtime GI walk can use the same cache as its terminator, which is
  what reference-lighting Q5 recommends.
- **PTR-RT-003.** The interactive PT mode MUST feed the existing SVGF and
  TAA chain with demodulated radiance, and the `DBG_DISABLE_ATROUS` A/B of
  the evaluation harness MUST keep working in that mode. When M54's neural
  spatial stage is selected it replaces à-trous in this mode as it does in
  the realtime path, so NDN-G-006 is measured here.
- **PTR-RT-004.** With the radiance cache disabled, the interactive PT
  mode's reuse MUST converge to the progressive reference on a frozen
  scene. The GRIS abstract says ReSTIR-style iterative reuse "loses most
  convergence guarantees" of RIS, derives convergence conditions, and
  presents a ReSTIR PT variant that guarantees convergence for offline
  rendering (Lin et al. 2022, [project page](https://graphics.cs.utah.edu/research/projects/gris/)).
  Whether that variant's conditions hold for the canonical BSDF and the
  engine's shift mapping is **Phase 0: verify** (V1); if they do not, this
  requirement is withdrawn in favour of a published bias figure.
- **PTR-RT-005.** A cinematic sequence MUST be renderable headless from a
  `renderer-stepped` camera path (`--bench-camera`), one output frame per
  fixed simulation step, with a stated spp per frame, to a numbered EXR
  sequence with one manifest.
- **PTR-RT-006.** The screen-sized and world-space buffers of both PT modes
  MUST each have a row in [memory-budget.md](../engine/memory-budget.md#restir-reservoirs)
  and MUST be allocated only while a PT mode is selected. The one exception
  is the radiance cache: if an approved revision of reference-lighting.md
  adopts it as the realtime terminator (its Q5), that PRD owns the cache's
  allocation rule and this requirement covers the path reservoirs and
  accumulation images only.
- **PTR-RT-007.** The interactive PT mode MUST trace at least two diffuse
  bounces before a path may terminate into the radiance cache, and its
  manifest MUST state the bounce limit, so ROADMAP M-LIGHT's "≥ 2 bounces in
  PT mode" (reference-lighting T14, owned here per RLT-NG-003) is checked by
  A10.

### 5.8 Tooling and explainability (`PTR-DX`)

- **PTR-DX-001.** `render.reference on|off|save <name>` MUST be available in
  `byro-dbg` beside `render.debug`, the structured correctness-view command
  it shares a frame contract with; `save` takes a bare filename and writes
  under `screenshots/` like `screenshot` does.
- **PTR-DX-002.** `render.reference status` MUST print the mode, spp,
  frames since reset, last reset cause, non-finite count, and every T2 and
  T3 class present with its pixel share.
- **PTR-DX-003.** The interactive PT mode SHOULD expose its radiance cache
  occupancy and reservoir rejection counts in `render.reference status`.

---

## 6. Deliberate deviations and risks

| # | Original or realtime behaviour | Redux PT behaviour | Becomes redundant | Risk and its requirement |
|---|---|---|---|---|
| 1 | Creation Kit lights have a type that may be "Omnidirectional (no shadow casting)" and a "Maximum light radius" (CK wiki, [Light](https://ck.uesp.net/wiki/Light); local dump `ck-uesp-wiki/(main)/Light.wiki`) | Every emitter casts RT shadows (already true of the realtime path, backbone item 4); the authored radius is kept through the shared attenuation curve | Nothing new | A reference with inverse-square physics would disagree with the realtime path by design → §10 Q6, PTR-MAT-004 |
| 2 | Realtime emissive surfaces light the scene only when a GI ray hits them (§3.1 row 2) | Emissive triangles are sampled lights (PTR-EST-003) | The Cornell `ceiling_light` workaround in the parity scene | Glow-mapped game content may light rooms brighter in the reference than authors intended. Whether the original engines let self-illuminated surfaces light other objects is **Phase 0: verify** (V12) |
| 3 | Realtime GI truncates at two diffuse bounces and 6 000 units | Unbounded with Russian roulette (PTR-EST-001) | — | Interiors converge brighter than the realtime look; this is the realtime path's error, reported by PTR-VAL-003, not a reason to truncate the reference |
| 4 | Realtime glass scales GI throughput by a fixed tint mix | Dielectric Fresnel interface (PTR-MAT-003) | — | Caustics through glass are slow to converge in a forward path tracer; the manifest's spp and the slope test (PTR-VAL-001) expose it |
| 5 | Water, particles, combustion media, decals, ground cover, fire-refraction cards and far LOD blocks render in the realtime path (§3.1 rows 10–12, 14, 16–18) | T3 in early phases | — | A reference that silently omits them looks plausible and wrong → PTR-CNT-005, PTR-OUT-004 |

---

## 7. Per-game scope and acceptance

### 7.1 Per-game scope

Both PT modes read only the canonical scene, so no title needs PT code of
its own; per-game differences are the translate step's
([NIFAL](../engine/nifal.md), [EXAL](../engine/exal.md)). The waves below
are about which titles' *content* is used for acceptance; which content
each title can load today is the "Verified content" column of the
[ROADMAP compatibility matrix](../../ROADMAP.md#compatibility-matrix),
linked rather than restated here.

| Game | In scope | Why / wave |
|---|---|---|
| Oblivion | yes | Wave 2: canonical content renders through the same path; an interior is in the matrix's verified content; in the R5 material-provider matrix |
| Fallout 3 | yes | Wave 2: shares FNV's translation; an interior is in the matrix's verified content |
| Fallout: New Vegas | yes | Wave 1: `scripts/renderer-eval-fnv.sh` Prospector Saloon is the existing real-content probe ([§ FNV probe](../engine/renderer-evaluation.md#fallout-new-vegas-real-content-probe)) |
| Skyrim SE / AE | yes | Wave 2: an interior is in the matrix's verified content; in the R5 material-provider matrix |
| Skyrim LE | yes, no acceptance scene | The matrix lists it as parse only; renders whatever reaches the canonical scene |
| Fallout 4 | yes | Wave 2: an interior is in the matrix's verified content; BGSM materials translate to the same canonical table |
| Fallout 76 | yes, no acceptance scene | The matrix lists it as parse only, so there is no cell to accept against |
| Starfield | yes, no acceptance scene | A walkable interior exists, but CDB per-field material extraction is missing, so material roles are honestly absent ([R5 status](../engine/rt-lighting-material-recovery.md#execution-status)) and a reference would measure that gap, not the estimator |

Wave 0 is the Cornell box, which needs no game data.

### 7.2 Acceptance

A scenario passes when every condition listed for it holds. Game data is
read from a local install through the existing environment variables
([testing.md § Game data resolution](../engine/testing.md#game-data-resolution));
no game file, extracted asset or game-derived image is checked in.

| # | Scenario | Command | Data | Pass when |
|---|---|---|---|---|
| A1 | Accumulation state machine | new `cargo test -p byroredux-renderer pt_reference_accumulation_resets_on_scene_change` | none | Each PTR-MODE-003 change resets spp to zero; no other input does |
| A2 | No game identity in PT shaders | new `cargo test -p byroredux-renderer pt_shaders_read_no_game_identity` | none | PTR-MAT-005 holds |
| A3 | Monte Carlo decay | new ignored test `cornell_pt_reference_error_decays_at_monte_carlo_rate` in `byroredux/tests/cornell_rt_oracle.rs` | RT device | PTR-VAL-001 slope within tolerance |
| A4 | External parity | new script `scripts/pt-reference-parity.sh` (exports the parity scene, renders it in the external renderer, renders the progressive reference, compares) | RT device; external renderer installed by the developer | PTR-VAL-002 relMSE within tolerance; deterministic export (PTR-VAL-004) |
| A5 | Realtime vs reference | `scripts/renderer-eval.sh` with the new reference case | RT device | Manifest carries the PTR-VAL-003 columns for every case |
| A6 | Real content | `scripts/renderer-eval-fnv.sh` with the new reference case | FNV install (`BYROREDUX_FNV_DATA`) | EXR written, non-finite count zero, T3 list and shares present |
| A7 | Live capture | `cargo run --release -- --cornell --bench-mode renderer-static --bench-hold` with `BYRO_DEBUG_SERVER=1`, then `byro-dbg`: `render.reference on`, `render.reference status` (repeat until target spp), `render.reference save ref.exr` | RT device | Status reports rising spp, then the target; the EXR and manifest exist under `screenshots/` |
| A8 | Multi-game smoke | new script `docs/smoke-tests/m51-pt-reference.sh` | Wave 2 installs; per-title `SKIP 77` when absent | One reference per title, zero non-finite samples, T3 shares reported |
| A9 | Cinematic sequence | A7's launch with `--bench-mode renderer-stepped --bench-camera orbit` and the sequence flag | RT device | Numbered EXRs and one manifest; frame count as requested |
| A10 | Interactive mode error | `scripts/renderer-eval.sh` interactive case | RT device | PTR-VAL-005 numbers present; manifest bounce limit ≥ 2 (PTR-RT-007); frame time within §10 Q7's budget |

Hardware scenarios follow ROADMAP's bench methodology (same machine,
pinned camera, a same-binary control) ([How to run a bench](../../ROADMAP.md#how-to-run-a-bench)).

---

## 8. Metrics

| Metric | Definition | Granularity |
|---|---|---|
| Reference parity error | relMSE of the progressive reference against the external renderer on the parity scene | per phase |
| Convergence slope | Least-squares log-log slope of relMSE against spp on Cornell L5 | per phase |
| Realtime estimator error | relMSE of the realtime final frame, and of each `DirectOnly` / `IndirectOnly` split, against the progressive reference | per bench scene, per release of the renderer-eval suite |
| Interactive estimator error | relMSE of the interactive PT mode against the progressive reference | per bench scene |
| T3 coverage gap | Pixels whose first hit is a T3 class ÷ all pixels | per bench scene, per class |
| Time to target spp | Wall time from accumulation start to the target spp at render resolution | per bench scene, on the bench-of-record machine |
| Interactive frame time | GPU time of the interactive PT mode per frame | per bench scene |

The numbers live in ROADMAP: Phase 1 adds a "Path-tracing reference" table
beside the [bench-of-record](../../ROADMAP.md#how-to-run-a-bench) and the
M51 row links it. This PRD never holds them.

---

## 9. Delivery sequence

**Phase 0 — Design, verify, decide (no engine code).** Write
`docs/engine/path-tracing.md` (the kernel, sampler, light sampling, the
pass placement after the TLAS build, the buffers and their memory rows).
Resolve §10 Q1–Q7; reference-lighting Q1 cites this PRD's Q5. Install the
chosen external renderer on the dev machine and record its version. Close
the verify list below. Fix the PTR-VAL-001 and PTR-VAL-002 tolerances.
Exit: the design is checked in, every V item is closed or filed as an
issue, every §10 item carries **Decided**, and the tolerances are written
into the design.

| # | Verify | Settled by |
|---|---|---|
| V1 | The GRIS / ReSTIR PT shift-mapping and convergence conditions on the canonical BSDF (PTR-RT-004), and which lobes lack an evaluable pdf (PTR-MAT-002) | Reading Lin et al. 2022 and its supplement against `include/pbr.glsl` |
| V2 | Whether a hash-grid cache written from Binder et al. 2019 and SHaRC's `docs/Integration.md` is clear of the RTX SDK licence | The owner's reading of the licence's §1 (grants) and §4 (limitations), or counsel |
| V3 | The NRC library's licence, API and CUDA dependence | Its licence file and integration guide; only if Q3 keeps NRC |
| V4 | The per-probe mapping of the Cornell materials onto the external renderer's BSDFs, and which probes form the parity scene | A white-furnace and single-bounce comparison per lobe |
| V5 | Whether `image` 0.25's `exr` feature writes 32-bit-float RGB | Its documentation and a scratch-crate round trip outside the workspace |
| V6 | Whether the combustion field can be sampled in world space outside the froxel grid | Reading `volumetrics_inject.comp` and `include/combustion_*.glsl` |
| V7 | The blended-surface treatment in ray paths | Reading `rayHitHasCoverage` and the blend draw path |
| V8 | Whether ground-cover blades have TLAS entries | **Closed in rev 2**: they do not (`groundcover_blade.frag` header); §3.1 row 14 is T3 |
| V9 | Whether the Cornell builders can spawn the scene without a `VulkanContext` for the exporter | Reading `byroredux/src/cornell/builders.rs`; shared with RLT-REF-001 |
| V10 | The water shading model's expressibility as a BSDF for PTR-CNT-002 | Reading `water.frag` and [WATAL](../engine/watal.md) |
| V11 | The per-submit GPU time ceiling on the dev driver | The driver's documentation or a measured device loss |
| V12 | Whether self-illuminated / glow-mapped surfaces lit other objects in the original engines | The GECK / CK wiki pages on glow maps, or nif.xml's shader-flag descriptions |
| V13 | Whether acceleration-structure builds and ray-query traversal are deterministic run to run on one device (PTR-EST-004) | The Vulkan specification's acceleration-structure chapter and two same-binary captures compared byte for byte |
| V14 | The source behind the ROADMAP M51 row's "Pharr et al. (SHARC, 2024)" reference | A citable paper or talk; otherwise the ROADMAP row drops it |

**Phase 1 — Progressive reference on the Cornell box.** First, as two
issues of their own that need nothing else from this phase, the
deterministic exporter (PTR-VAL-004) and the EXR writer (PTR-OUT-001);
reference-lighting's Phase 1 depends on those two issues, not on this
exit. Then the camera-ray path
tracer with NEE + MIS over `GpuLight` and emissive triangles, the full lobe
set at every vertex, Russian roulette, accumulation and reset, EXR +
manifest, `render.reference` (PTR-MODE-001…007, PTR-EST-001…006,
PTR-MAT-001…005, PTR-OUT-001…005, PTR-DX-001, PTR-DX-002). Depends on M37
and M37.3 (shipped). Exit: A1, A2, A3 and A7 pass on the RTX 4070 Ti, and
the convergence slope is published in ROADMAP.

**Phase 2 — Trusted reference.** The external comparison on the exported
parity scene (PTR-VAL-002), then the realtime
comparison columns in the renderer-eval suite (PTR-VAL-003). Exit: A4 and
A5 pass; the reference parity error and the realtime estimator error for
every Cornell case are published in ROADMAP. From this exit the converged
Cornell capture is available to reference-lighting's RLT-REF-005 and as
M54's R1 tier on the Cornell scenes.

**Phase 3 — Real content.** Skinned actors (PTR-CNT-001), T3 reporting
(PTR-CNT-005), the FNV probe and the multi-game smoke (Wave 1 then Wave 2).
Exit: A6 and A8 pass for FNV, then for every Wave 2 title with an install;
the T3 coverage gap per scene is published.

**Phase 4 — Remaining content classes.** Water (PTR-CNT-002), particles
(PTR-CNT-003), media (PTR-CNT-004) as V6 and V10 allow. Depends on WATAL
and the fog design's follow-up 5. Exit: the T3 coverage gap of the Wave 1
scenes falls to the classes V6 / V10 left out and the raster-only classes
of §3.1 rows 14 and 16–18, with each remaining class filed as an issue.

**Phase 5 — Interactive PT mode.** ReSTIR PT path reservoirs, the
hash-grid radiance cache, the denoiser hook, cinematic sequences
(PTR-MODE-008, PTR-RT-001…007, PTR-DX-003). Exit: A9 and A10 pass; the
interactive estimator error and frame time for the Cornell and FNV benches
are published in ROADMAP. From this exit NDN-G-006 can be measured
(M54 Phase 4) and the radiance cache exists for reference-lighting Q5's
terminator.

---

## 10. Decisions required

Each needs the project owner. The recommendation is the first option.

1. **Q1 — What M51 is.** Recommended: both products in order — the
   progressive reference first (Phases 1–4), the interactive PT mode
   second (Phase 5) — because ReSTIR PT and a radiance cache are biased
   accelerators whose error cannot be measured without the reference.
   Alternative A: interactive PT only, as the ROADMAP row's technique list
   reads; consequence: no ground truth, and M-LIGHT's "≥ 2 bounces in PT
   mode" ([reference-lighting.md](reference-lighting.md) T14) and M54's
   R1 training target ([neural-denoiser.md](neural-denoiser.md)) rest on a
   biased image. Alternative B:
   progressive reference only, with the interactive mode moved to a new
   milestone; consequence: M51 ships sooner and the ROADMAP row's ReSTIR
   PT / SHARC scope is rewritten.
2. **Q2 — Radiance-cache provenance.** Recommended: a clean-room hash-grid
   cache in GLSL written from Binder et al. 2019 and the published SHaRC
   integration description, so no RTX SDK source enters the MIT tree.
   Alternative A: vendor NVIDIA SHaRC; consequence: its licence §4(e)
   conflicts with the MIT licence and its supplement §1 ties the
   application to NVIDIA hardware interoperability (PTR-NG-004 would be
   withdrawn). Alternative B: no cache, deeper paths with ReSTIR PT only;
   consequence: higher interactive frame time, simpler code, and
   reference-lighting Q5's recommended terminator does not exist (its
   Alternative A or B applies). V2 must close before this is decided.
   Whichever option is taken, reference-lighting Q5's "SHARC" reads as
   "M51's hash-grid radiance cache" (§2).
3. **Q3 — NRC.** Recommended: out of M51 (PTR-NG-003), revisited after M54
   builds a Vulkan-compute inference path. Alternative A: in M51 with an
   engine-written MLP trained in compute; consequence: M51 absorbs M54's
   infrastructure work. Alternative B: NVIDIA's NRC library; consequence:
   a proprietary binary dependency (V3) against PTR-NG-004 and the M54 row's
   "no proprietary SDK dependency".
4. **Q4 — Primary visibility.** Recommended: camera rays issued by ray
   query from a compute pass in both PT modes (PTR-MODE-002), which gives
   in-estimator anti-aliasing and depth of field and keeps blended surfaces
   out of raster order. Alternative A: reuse the raster G-buffer's primary
   hit in the interactive mode; consequence: cheaper, but the mode is no
   longer "full PT" as the ROADMAP row states, and blended content keeps a
   raster-order dependence. Alternative B: adopt `VK_KHR_ray_tracing_pipeline`
   for both; consequence: a new device requirement and a second ray
   dispatch model beside ray query.
5. **Q5 — External reference renderer.** Recommended: Mitsuba 3, whose
   `principled` BSDF follows Burley's Disney papers, the lineage of the
   engine's lobe ([Mitsuba 3 BSDFs](https://mitsuba.readthedocs.io/en/stable/src/generated/plugins_bsdfs.html)),
   and whose pip wheels include CPU `scalar_rgb` / `llvm_ad_rgb` variants
   that run on Linux without an NVIDIA GPU ([variants](https://mitsuba.readthedocs.io/en/stable/src/key_topics/variants.html)).
   Alternative: PBRT-v4, which writes OpenEXR / PFM but has no Disney
   material — its documented set is coateddiffuse, coatedconductor,
   conductor, dielectric, diffuse, diffusetransmission, hair, interface,
   measured, mix, subsurface and thindielectric ([pbrt-v4 file format](https://pbrt.org/fileformat-v4));
   consequence: the parity scene shrinks to diffuse walls, smooth glass and
   conductor probes. Both are developer-installed tools, never engine
   dependencies. This decision is this PRD's alone; reference-lighting Q1
   cites it, so the project carries one external renderer and one
   exporter (PTR-VAL-004).
6. **Q6 — Legacy attenuation in the reference.** Recommended: evaluate the
   uploaded `AttenuationModel` as the realtime path does (T2 for
   `LegacySoftRange`), so the reference measures estimator error only; the
   parity scene uses emissive-panel lighting and no `GpuLight`, so the
   external comparison is unaffected. Alternative: force `InverseSquare` in
   the reference; consequence: realtime-vs-reference differences then mix
   model and estimator error and PTR-G-002 loses its meaning.
7. **Q7 — Interactive frame budget.** Recommended: an interactive, not
   playable, target — 33 ms GPU time per frame at 1080p render resolution
   on the dev RTX 4070 Ti for the Cornell and FNV benches. Alternative:
   no budget, quality-first; consequence: Phase 5's exit has no frame-time
   gate and PTR-G-004 measures error only.

---

## 11. Document map

| Need | Go to |
|---|---|
| Capture workflow, manifests, determinism contract | [renderer-evaluation.md](../engine/renderer-evaluation.md) |
| Emitters, visibility layers, ray budget | [physical-lighting-backbone.md](../engine/physical-lighting-backbone.md) |
| Cornell L0–L5 oracle and the RT CI workflow | [rt-lighting-material-recovery.md § R4](../engine/rt-lighting-material-recovery.md#r4---build-the-cornell-l0-l5-oracle) |
| Media and their follow-up boundary | [procedural-volumetric-fog.md](../engine/procedural-volumetric-fog.md#follow-up-boundary) |
| BSDF, SVGF, TAA, acceleration structures | [renderer.md](../engine/renderer.md) |
| GPU data types and pass order | [shader-pipeline.md](../engine/shader-pipeline.md) |
| Buffer budgets | [memory-budget.md](../engine/memory-budget.md) |
| Per-game tiers | [game-compatibility.md](../engine/game-compatibility.md) |
| Game-data environment variables | [testing.md](../engine/testing.md#game-data-resolution) |
| Realtime lighting gaps, the exporter's rung content, the component-decomposition contract, the radiance-cache terminator (M-LIGHT) | [reference-lighting.md](reference-lighting.md) |
| Reference tiers R0–R2 and the neural spatial stage (M54) | [neural-denoiser.md](neural-denoiser.md) |
| Per-game verified content | [ROADMAP compatibility matrix](../../ROADMAP.md#compatibility-matrix) |
| Live milestone state and published metrics | [ROADMAP.md](../../ROADMAP.md) |

---

## 12. Revision and review log

| Rev | Date | Change | Review |
|---|---|---|---|
| 1 | 2026-11-01 | First draft | — |
| 2 | 2026-11-01 | Review round 1 folded in: console-command inventory (PTR-MODE-001), missing TLAS exclusions, GI clamps and interior ambient in §3.1, V8 closed, determinism softened (PTR-EST-004, V13), per-game scope linked to the ROADMAP matrix, SHaRC licence clauses, GRIS convergence wording, verify list as a table, cross-PRD alignment with reference-lighting and neural-denoiser (PTR-RT-003, PTR-RT-006, PTR-RT-007, PTR-VAL-004, Q2, Q5); coordinator cross-PRD sync (sibling links and citations) | claude: revise |
| 3 | 2026-11-01 | Single ownership, from reference-lighting's review round 2: the exporter (PTR-VAL-004) and the EXR writer (PTR-OUT-001) are owned here and filed as their own Phase 1 issues ahead of the progressive reference; Q5 is decided here and cited by reference-lighting Q1; PTR-VAL-003 points at reference-lighting's component-decomposition contract | — |
| 4 | 2026-11-02 | Accepts reference-lighting's change requests RLT-EXP-001…006 (skinned pose at a frame index, normal-map and alpha textures, sun-free analytic environment, sun as a distant emitter, export-time *k*<sub>exp</sub> emitter scaling, per-rung overrides) into PTR-VAL-004's exporter scope. Per the owner's reference-lighting Q1 decision (2026-11-02), the exporter (PTR-VAL-004) and the EXR writer (PTR-OUT-001) are built now as their two stand-alone Phase 1 issues, ahead of this PRD's approval; nothing else of M51 is | — |
