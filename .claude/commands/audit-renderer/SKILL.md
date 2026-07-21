---
description: "Deep audit of the Vulkan renderer — pipeline, sync, memory, shaders, ray tracing, denoiser"
argument-hint: "--focus <dimensions> --depth shallow|deep"
---

# Renderer Audit

Audit the Vulkan renderer for correctness across the full pipeline: ray tracing
(BLAS/TLAS, ray queries, shadows, reflections, GI, glass refraction), SSBO/UBO
indexing, GPU-struct layout, synchronization, GPU memory, deferred indirect
lighting (G-buffer, SVGF), denoiser/composite, and the per-feature passes
(TAA, skinning, caustics, water, volumetrics, bloom, material table).

**Architecture**: Orchestrator. Each dimension runs as a Task agent (max 3 concurrent).

See `.claude/commands/_audit-common.md` for project layout, the **Key Reference
Docs** table, methodology, dedup, context rules, and the finding format.
See `.claude/commands/_audit-severity.md` for the severity scale — the
RT/SSBO/GPU-struct/denoiser rows there set the floors used below.

**Do NOT restate the GPU-struct byte layouts, descriptor bindings, G-buffer
formats, or submission order here** — `docs/engine/shader-pipeline.md` is the
authoritative, code-verified reference. `docs/engine/memory-budget.md` is
authoritative for VRAM/RAM ceilings, LRU thresholds, and deferred-destroy depth.
Audit *against* those docs; if the code diverges from the doc, that divergence is
itself a finding (or the doc is stale — note which).

## Verification discipline (No-Guessing)

- **Symbols, not line numbers.** Anchor every finding on a symbol
  (`fn`/`struct`/`const`/test name), not `file:NN` — line anchors rot on every
  refactor. Confirm by `grep`; if a claim is unconfirmable, drop it.
- **Backticked `.ext` paths must resolve now** (the `_audit-validate.sh` gate).
  note that `byroredux/src/render/` is a **directory**, not a `render.rs` file
  (it split out post-#1115); `scene.rs` / `systems.rs` / `cell_loader.rs` are
  likewise thin dispatchers over sibling dirs.
- **Recast resolved issues as regression guards** — phrase as "verify X still
  holds / hasn't drifted", not "X is broken".
- **No speculative Vulkan changes.** Per the user's standing guidance, do NOT
  propose render-pass / pipeline / barrier edits whose failure modes are
  invisible to `cargo test`. Frame such findings as **"needs RenderDoc
  verification"** and stop at the observation.
- **Bench numbers rot.** Do not hard-code FPS/ms; cite ROADMAP.md
  *Bench-of-record* (currently flagged stale — R6a-stale-15 gates any FPS claim).

## Parameters (from $ARGUMENTS)

- `--focus <dimensions>`: Comma-separated dimension numbers (e.g., `1,3,7`). Default: all.
- `--depth shallow|deep`: `shallow` = check patterns only; `deep` = trace data flow and validate invariants. Default: `deep`.

## Extra Per-Finding Fields

- **Dimension**: AS Correctness | SSBO/Indexing | GPU-Struct Layout | Sync/Barriers | Memory/Lifecycle | Ray Queries | Denoiser/Composite | TAA | Skinning | Camera-Relative Precision | NIFAL Material | Material Table | Caustics | Water | Volumetrics | Bloom | Disney BSDF | Soft Shadows | Sky/Weather | Tangent-Space | Pipeline/RenderPass | Debug/Telemetry | Cornell Harness | Light Animation

## Phase 1: Setup

1. Parse `$ARGUMENTS` for `--focus`, `--depth`.
2. `mkdir -p /tmp/audit/renderer`.
3. Dedup baseline: `gh issue list --repo matiaszanolli/ByroRedux --limit 200 --json number,title,state,labels > /tmp/audit/renderer/issues.json`.
4. Scan `docs/audits/` for prior renderer reports.
5. Read `docs/engine/shader-pipeline.md` + `docs/engine/memory-budget.md` first — they pin almost everything below.

## Phase 2: Launch Dimension Agents

Dimensions are ordered **by renderer risk**: AS/SSBO indexing and GPU-struct
layout corrupt every frame silently (CRITICAL/HIGH floors); sync and memory
leaks compound; denoiser/shader correctness is mostly visual.

---

### CRITICAL tier — silent whole-frame corruption

#### Dimension 1: Acceleration Structures (BLAS/TLAS correctness)
**Entry points**: `crates/renderer/src/vulkan/acceleration/` (`blas_static.rs`, `blas_skinned.rs`, `tlas.rs`, `predicates.rs`, `constants.rs`, `types.rs`), `crates/renderer/src/vulkan/context/resources.rs` (`build_blas_for_mesh`).
**Severity floor**: wrong geometry/address in AS = CRITICAL; missing build→read barrier = HIGH.
**Checklist**:
- BLAS build geometry: vertex format `R32G32B32_SFLOAT` at offset 0, index type `UINT32`, `OPAQUE` flag, correct prefer-trace/build flags per buffer class.
- Build-flag constants are stable: `STATIC_BLAS_FLAGS` (`FAST_TRACE | ALLOW_COMPACTION`), `SKINNED_BLAS_FLAGS` (`FAST_BUILD | ALLOW_UPDATE` — deliberate, see memory-budget.md), `UPDATABLE_AS_FLAGS` (`FAST_TRACE | ALLOW_UPDATE`). Pinned by tests in `acceleration/`; drift = Vulkan-version-rev breakage (regression guard, #1144/#1196).
- `BlasEntry.built_flags` records BUILD-time flags; refit must assert the same set (VUID-03667). Mismatch surfaces as validation, not silent corruption (regression guard, #1145).
- **`instance_custom_index` encoding == draw-command index** used for SSBO lookup — this is the load-bearing AS/SSBO contract (CRITICAL). It is a 24-bit field; `MAX_INSTANCES = 0x40000` stays under `1 << 24`, pinned by the const-assert in `scene_buffer/constants.rs`.
- TLAS build/update decision keys on the `last_blas_addresses` device-address sequence only. UPDATE mode requires matching geometry + instance count — verify padded/unused instance slots don't break it.
- Transform: column-major `mat4` → 3×4 row-major `VkTransformMatrixKHR`. `TRIANGLE_FACING_CULL_DISABLE` on all instances (two-sided meshes).
- Empty TLAS valid from frame 0 (no validation errors before any geometry).
- Device-address queries require `SHADER_DEVICE_ADDRESS` usage on the source buffer.
- LRU/shrink wiring (regression guards): `shrink_tlas_scratch_to_fit` uses TLAS-calibrated slack matching `tlas_instance_should_shrink` (`predicates.rs`), called at cell-unload (#1226); the three `missing_blas` cause-counters (skinned/rigid/ssbo_evicted) all increment and surface via `mem.stats` (#1228); post-TLAS `rt_flag` patch in `draw_frame` keeps cell-load frames from rendering RT-disabled (#1227). Two known, documented-not-fixed correctness gaps live here (#1793): a permanently-missing rigid BLAS has no recovery path (no per-frame build primitive exists), and a synchronous multi-cell burst (`--grid`) can false-evict a not-yet-drawn entry via the shared `frame_counter` bump — both gated behind `static_blas_bytes > budget`, unreachable on the 12 GB dev card. Recast, don't re-report as new.
- **Deferred BLAS destruction (regression guard, #a476b256).** `drop_blas` / `evict_unused_blas` (`blas_static.rs`) and the skinned drop (`blas_skinned.rs`) push the `VkAccelerationStructureKHR` + backing buffers onto `pending_destroy_blas` (deferred, `DEFAULT_COUNTDOWN` frames) instead of destroying immediately — an eviction or unload must not free an AS the in-flight frame's ray queries still read (use-after-free → CRITICAL). The shutdown path drains `pending_destroy_blas` synchronously. Regression = a re-introduced immediate `destroy_acceleration_structure` at the eviction/drop site.
**Output**: `/tmp/audit/renderer/dim_1.md`

#### Dimension 2: SSBO/Index plumbing & RT ray queries (shader)
**Entry points**: `crates/renderer/shaders/triangle.frag` + its `#include`d `crates/renderer/shaders/include/raytrace.glsl` / `include/lighting.glsl` (all `rayQueryEXT`), `crates/renderer/shaders/water.frag`.
**Severity floor**: SSBO index mismatch = CRITICAL; ray self-intersection / wrong tMin = HIGH.
**Checklist**:
- `instance_custom_index` (NOT `gl_InstanceID`) indexes `GpuInstance[]`; `materials[instance.material_id]`, vertex/index SSBOs (Set 1 bindings 8/9 per shader-pipeline.md) use the same offsets the Rust upload writes.
- Shadow rays: origin = surface world pos with normal/tMin bias, direction toward light, `TerminateOnFirstHit`, `CommittedIntersectionNone` → 0/1. Disk/cone jitter geometry correct (point/spot concentric disk, directional angular cone).
- Reflection rays: normal-biased origin, `reflect(viewDir, N)` sign, metalness/roughness gate consistent with PBR intent, barycentric UV interp from vertex SSBO, descriptor-valid texture lookup.
- 1-bounce GI: cosine-weighted hemisphere with correct tangent-basis, distance cutoff, miss → sky/ambient fill with no NaN/inf.
- Glass / IOR refraction:
  - Roughness-spread basis via **Frisvad** orthonormal basis (not `cross(N, up)`, which degenerates vertical) — verify tangent/bitangent unit length (#820).
  - Window-portal demote on coincident glass to break the IOR self-passthrough infinite loop (#789).
  - `GLASS_RAY_BUDGET` (from `shader_constants.glsl`) cap wired; the budget `atomicAdd` overshoots unconditionally by design (#1438) — that's documented, not a bug; verify the doc comment is intact and no CPU reads the counter.
  - Interior miss falls back to cell-ambient, not open-sky tint (no daylight leak in dungeons).
  - `DBG_VIZ_GLASS_PASSTHRU` viz still wired at the diagnostic-state setup + the two refraction-loop viz-write branches.
  - **Thin-glass gate (regression guard, #883f57cd).** `MAT_FLAG_THIN_GLASS` (bit 11) forces non-occluding glass (open window panes, display-case fronts) onto the zero-ray Fresnel/framebuffer-transmission path: `glassIORAllowed = isGlass && !isThinGlass && rtEnabled && !isWindow && rtLOD < RT_LOD_IOR`. Occluding/thick glass (bottles, canopies) keeps the full Snell/RT path. BGEM classification pinned by `bgem_uses_thin_glass_behavior` / `closed_bgem_glass_does_not_select_thin_surface_behavior` / `legacy_bgem_effect_cards_do_not_become_glass` (`asset_provider/tests.rs`) — thin only for non-occluding transmissive shells, never plain closed BGEM glass or effect cards.
- RT gating: `sceneFlags.x > 0.5` checked before every ray query; TLAS binding is the correct descriptor (Set 1, Binding 2).
- Interleaved-gradient noise seeded by frame counter — deterministic per-pixel-per-frame so TAA can converge (no true RNG).
- **ReSTIR-DI spatial reuse (regression guard, #d523b9b3).** The shadow-reservoir spatial pass in `triangle.frag` rejects a neighbour reservoir on a 25° geometric-normal cone (`SPATIAL_NORMAL_COS = 0.906`, Bitterli 2020 §5) BEFORE combining — the neighbour's geometric normal is octEncode→packSnorm2x16-packed into the reservoir `pad0` at write time (no normal-history texture, reservoir stays 32 B). It uses the GEOMETRIC normal (`fragNormalEffective`), not the normal-mapped shading N (so the cone doesn't over-reject on bumpy detail). Gated by `DBG_DISABLE_SPATIAL` for A/B; stale/uninit reservoirs decode to a degenerate normal the gate rejects. Regression: dropping the normal-cone test (re-opens cross-corner shadow bleed), packing the shading N, or growing the reservoir struct.
- **ReSTIR-DI surface-identity tag now uses the stable surface ID (regression guard, #883f57cd).** The reservoir's surface tag is `uint surfaceId = inst.surfaceId & RESERVOIR_SURFACE_MASK` (mask `0x3FFFFF`), replacing the old `fragInstanceIndex + 1` — so spatial-reuse validity survives per-frame draw-order/batch reordering instead of going stale whenever the draw list re-sorts. Test: `restir_history_uses_stable_surface_id_not_instance_order` (`gpu_instance_layout_tests.rs`).
- BC1 punch-through alpha (regression guard, #ae285062): a pure-blend mesh whose BC1/DXT1 diffuse decodes index-3 texels as `a==0` (an RGB-fidelity encoder choice, NOT transparency) must NOT leak into blend-discard / decalWeight / finalAlpha. `triangle.frag` pins `texColor.a = 1.0` when `INSTANCE_FLAG_DIFFUSE_ALPHA` (bit 8) is clear and no alpha test is active. The CPU bit is set in `draw.rs` from `format_has_alpha` (which excludes `BC1_RGBA`). Regression = a BC1-blend mesh speckling/pinholing again.
**Output**: `/tmp/audit/renderer/dim_2.md`

#### Dimension 3: GPU-struct layout (lockstep with shaders)
**Entry points**: `crates/renderer/src/vulkan/scene_buffer/gpu_types.rs`, `scene_buffer/constants.rs`, `crates/renderer/src/vulkan/material.rs`, the layout-pin tests in `scene_buffer/` (`gpu_instance_layout_tests.rs`, `material_hash_tests.rs`, `instance_hash_tests.rs`) and `material.rs`.
**Severity floor**: `#[repr(C)]` GPU struct drifting from its shader struct = HIGH (silent per-instance/per-material corruption).
**Checklist**:
- Sizes pinned by tests — confirm they hold and match shader-pipeline.md: `GpuInstance` = **112 B** (`size_of::<GpuInstance>() == 112`), `GpuCamera` = **336 B** (`gpu_camera_is_336_bytes` — grew 320→336 with the `render_origin` vec4, #markarth-precision / #1492), `GpuMaterial` = **300 B** (`gpu_material_size_is_300_bytes`; NOTE the size grew 260→…→300 via #804/#1249/#1250 — the `_300_` test name is current, the old `_260_` name is gone).
- **`GpuInstance.surface_id`** (`u32`, offset 108, regression guard #883f57cd) repurposes the old `_pad_albedo` padding into a stable per-entity surface identity (`draw_cmd.entity_id.wrapping_add(1)` — 0 reserved for background/synthetic), used by TAA/SVGF disocclusion and ReSTIR-DI reuse to survive per-frame draw-order reshuffling. Size stays 112 B. Pinned by `gpu_instance_field_offsets_match_shader_contract` (asserts offset 108) plus `restir_history_uses_stable_surface_id_not_instance_order` / `gbuffer_history_uses_stable_surface_id_but_caustics_keep_draw_lookup`.
- Per-field offset pins: `gpu_material_field_offsets_match_shader_contract` asserts every named field offset across all vec4 slots (#806) — a size-only pin can't catch within-vec4 reorders. Any added field needs a matching offset assertion plus updates to the Rust struct AND the GLSL `struct GpuMaterial` (only declared in `crates/renderer/shaders/include/bindings.glsl`, `#include`d by `triangle.frag`).
- All `GpuMaterial` fields are scalar f32/u32 — **never** `[f32;3]` (std430 vec3 alignment would desync the byte-hash dedup). Named pad fields explicitly zeroed (no uninit bytes feeding Hash/Eq).
- `struct GpuInstance` is declared once in `crates/renderer/shaders/include/bindings.glsl` (pulled into `triangle.frag` via `#include`) and **hand-mirrored in 4 standalone shaders** — verify lockstep via `grep -rl "struct GpuInstance" crates/renderer/shaders/` → `include/bindings.glsl`, `triangle.vert`, `ui.vert`, `water.vert`, `caustic_splat.comp` (5 declaration sites; `ui.vert`/`water.vert` reading wrong offsets is the recurring trap, #785/#1498). Every mirror must carry the `surfaceId` field (renamed from the old albedo pad, #883f57cd) — `grep -L surfaceId` across the 5 sites should return nothing. Per `feedback_shader_struct_sync.md`.
- Flag constants are the single source of truth in `crates/renderer/src/shader_constants_data.rs`, emitted into `crates/renderer/shaders/include/shader_constants.glsl` and `#include`d — never hand-written shader-side: `INSTANCE_FLAG_*` (`NON_UNIFORM_SCALE` bit 0, `ALPHA_BLEND` bit 1, `CAUSTIC_SOURCE` bit 2, `TERRAIN_SPLAT` bit 3, `FLAT_SHADING` bit 7, `DIFFUSE_ALPHA` bit 8 — the BC1 punch-through gate, #ae285062), `MATERIAL_KIND_*`, `MAT_FLAG_*` (bits 0–9, including `PBR_BSDF` bit 5, `TRANSLUCENCY` bit 6, `MODEL_SPACE_NORMALS` bit 7, `TRANSLUCENCY_THICK_OBJECT` bit 8, `TRANSLUCENCY_MIX_ALBEDO` bit 9 — all canonical post-#1357, the `BGSM_*` prefix is gone; plus `THIN_GLASS` bit 11 — bit 10 unused/reserved — gating occluding-vs-non-occluding glass, #883f57cd), and the 13 `DBG_*` bits (`0x1`…`0x1000`, value-pinned by the shared catalog, `8eaade44`).
- Capacity constants match memory-budget.md: `MAX_INSTANCES = 0x40000`, `MAX_MATERIALS = 16384` (`scene_buffer/constants.rs`), `MAX_INDIRECT_DRAWS = MAX_INSTANCES`. Over-cap material intern returns id 0 + one-shot `warn!` (no SSBO-index corruption, #797); upload truncates to `min(intern_count, MAX_MATERIALS)`.
**Output**: `/tmp/audit/renderer/dim_3.md`

---

### HIGH tier — sync, memory, leaks, denoiser correctness

#### Dimension 4: Synchronization & barriers
**Entry points**: `crates/renderer/src/vulkan/context/draw.rs` (`draw_frame`), `sync.rs`, `context/resize.rs`. Cross-check the submission order in shader-pipeline.md.
**Checklist** (flag invisible-failure-mode items as **needs RenderDoc**):
- Semaphore/fence lifecycle: signal-before-wait, no double-signal, per-frame fence waited before command-buffer reuse, `images_in_flight` tracking.
- `render_finished` is **per-swapchain-image, indexed by `image_index`** (not per-frame) — per-frame signalling fires VUID-vkQueueSubmit-pSignalSemaphores-00067 when `MAX_FRAMES_IN_FLIGHT` > swapchain image count (regression guard, `548c1b69`).
- AS build → fragment read barrier (`AS_WRITE → AS_READ`, build stage → fragment stage); skin compute write → BLAS refit → fragment read chain (Dim 9).
- **AS-build INPUT barrier access flag (regression guard, #507945d8).** Vertex/index/instance build *inputs* are read with `SHADER_READ` at the `ACCELERATION_STRUCTURE_BUILD` stage — NOT `ACCELERATION_STRUCTURE_READ_KHR` (that flag is for reading an AS *structure*, not its build inputs). Applies to the instance-buffer-copy → TLAS-build barrier (`tlas.rs`) and the skinned-vertex compute-write → BLAS-build barrier (`draw.rs`). The wrong flag is a RAW hazard surfaced by sync-validation (~40 hazards/frame on `--cornell` pre-fix); turn validation on via `BYRO_VALIDATION` (release) to confirm.
- G-buffer attachment transitions between render pass and the compute consumers (SVGF/TAA/SSAO); caustic-accum atomic-add → SHADER_READ.
- egui render pass supplies its own incoming dependency after composite's outgoing `dstStage = NONE` (explicit EXTERNAL dependency, #1433) — missing it is a WAR hazard on the swapchain image.
- Swapchain recreate: all in-flight work waited and resources destroyed before rebuild.
**Output**: `/tmp/audit/renderer/dim_4.md`

#### Dimension 5: GPU memory & resource lifecycle
**Entry points**: `crates/renderer/src/vulkan/buffer.rs`, `allocator.rs`, `scene_buffer/`, `acceleration/memory.rs`, `crates/renderer/src/vulkan/context/mod.rs` (Drop), `context/resize.rs`. Cross-check ceilings in memory-budget.md.
**Severity floor**: any per-frame leak = HIGH.
**Checklist**:
- gpu-allocator memory-type correctness (`CpuToGpu` vs `GpuOnly`); buffers/images destroyed before allocator; allocator dropped before device; no leaked `VkDeviceMemory` on shutdown.
- **`AllocatorResource` ECS-ordering**: must be removed from the `World` BEFORE `VulkanContext::drop()` — the allocator holds a live `Arc<Device>`; a `World` outliving the context fires the allocator Drop against a destroyed device (use-after-free). Verify the drop/remove ordering in `main.rs`, and that it survives a panic-unwind path (#1406). Allocator-independent destroys are hoisted out of the allocator-guarded Drop block (#1483).
- BLAS scratch high-water-mark reuse never shrinks mid-life (verify no use-after-free); shrink fns (`shrink_blas_scratch_to_fit`, `shrink_tlas_to_fit`) run at cell-unload with the slack constants from memory-budget.md.
- **Deferred BLAS-scratch destruction (regression guard, #1782).** The shared `blas_scratch_buffer` retired on grow/shrink routes through `pending_destroy_scratch: DeferredDestroyQueue<GpuBuffer>` (deferred, mirrors `pending_destroy_blas`) instead of an immediate free — the immediate-destroy at these cell-unload/streaming-worker sites was a GPU use-after-free (a just-submitted frame's skinned-BLAS refit/first-sight build may still read the old address). NOTE: `build_skinned_blas_batched_on_cmd`'s own grow-destroy stays immediate by design (runs after that frame's own fence wait) — don't "fix" it to match.
- TLAS resize calls `device_wait_idle()` before `allocator.free()` of the old allocation (`tlas.rs`) — absence opens a use-after-destroy window under resize-while-build-in-flight (latent, #1390).
- Vertex/index pool growth: soft cap `warn!`, hard cap error (`check_pool_growth`); `NifImportRegistry` LRU cap (`BYRO_NIF_CACHE_MAX`, default 2048) bounds scene count.
- Deferred-destroy countdown = `MAX_FRAMES_IN_FLIGHT` frames, ticked after the in-flight fence wait (memory-budget.md); BGSM/failed-path caches half-evict on overflow (#1430).
- Reverse-order teardown of all `VulkanContext` fields; per-subsystem `destroy()` for `AccelerationManager`, `SvgfPipeline`, `GBuffer`, `CompositePipeline`, `Ssao`, `WaterCausticAccum`, `EguiPass`, `GpuPerFrameTimers`, `TextureRegistry`; framebuffers before render pass, image views before swapchain, device last.
**Output**: `/tmp/audit/renderer/dim_5.md`

#### Dimension 6: NIFAL material canonical translation
**Entry points**: `byroredux/src/material_translate.rs` (`translate_material` — the single `ImportedMesh → Material` boundary), `crates/core/src/ecs/components/material.rs` (`Material`, `resolve_pbr`, `EmissiveSource`), the particle slice `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params`/`extract_emitter_rate`) → `byroredux/src/systems/particle.rs` (`apply_emitter_params`) → `byroredux/src/render/particles.rs` (`emit_particles`). Spec: `docs/engine/nifal.md`. See also `/audit-nifal`.
**Severity floor**: wrong/divergent `Material` out of `translate_material` = HIGH (one boundary, all-game blast radius, no per-draw fallback to mask it).
**Checklist**:
- **Single boundary**: `translate_material` has exactly two callers — `byroredux/src/scene/nif_loader.rs` (loose NIF) and `byroredux/src/cell_loader/spawn.rs` (REFR placement). A third `Material {…}` literal downstream is a translation leak.
- `Material::metalness` / `roughness` are plain resolved `f32` (not `Option`); `resolve_pbr` runs once at translate (NaN-sentinel → `classify_pbr_keyword`, then clamp `metalness 0..1`, `roughness 0.04..1`), idempotent (`resolve_pbr_is_idempotent`). No per-frame re-classification anywhere.
- `EmissiveSource` (None/Material/Lighting/Effect) resolved at translate; the renderer reads the resolved `emissive_mult`, not the raw per-game property (Effect = diffuse-tint multiplier conflated into emissive — drift mis-tints FO4+ glow).
- **No per-game branch between `Material` and `MaterialTable::intern`** — per-game quirks resolve here, never in the renderer (`feedback_format_translation.md`).
- Particle slice: authored emitter rate/size override the preset but NOT color (`apply_emitter_params_overrides_kinematics_and_size_not_color`); render assembly reads `ParticleEmitter` post-overlay.
**Output**: `/tmp/audit/renderer/dim_6.md`

#### Dimension 7: Material table (R1 dedup)
**Entry points**: `crates/renderer/src/vulkan/material.rs` (`MaterialTable::intern`), `scene_buffer/upload.rs`, `byroredux/src/render/mod.rs` (`build_render_data`), `byroredux/src/render/static_meshes.rs`.
**Upstream**: the interned `Material`s come from Dim 6 (NIFAL) — a corrupt `GpuMaterial` may be a translate-side bug.
**Checklist**:
- `intern` produces stable `material_id`s within a frame; identical materials collapse to one entry. Over-cap returns id 0 + warn-once, surfaced via `mem.stats` (#7823eb59/#797). Per-frame SSBO sized to `min(intern_count, MAX_MATERIALS)`.
- Hash/Eq treat `GpuMaterial` as raw bytes (depends on the Dim-3 scalar-fields + zeroed-pad invariant).
- Dedup-ratio telemetry surfaced (unique vs placement count) — a drop in hit-rate is a finding even if correctness holds (#780).
- Import-side scalars feeding the table (regression guards): BSLightingShaderProperty smoothness/IOR/specular into `MaterialInfo` for the Disney lobe, BGSM smoothness normalized once (no double-apply, #1241); WaterShaderProperty + bare BSShaderProperty produce distinct entries (no dedup collapse with glass/opaque, #1243/#1244); `HasModelSpaceNormals` routed for direct-TXST REFRs (#972).
- Identity invariant: N copies of one material render byte-identical pre/post dedup. No per-instance field remains in `GpuInstance`/`DrawCommand` that should now live in `GpuMaterial` (R1 Phase 6 closeout).
- **Particle color-fade quantization (regression guard, #1795).** `emit_particles` (`byroredux/src/render/particles.rs`) snaps the color LERP's fade parameter to 32 steps (`quantize_fade`, `COLOR_FADE_STEPS`) before hashing into `GpuMaterial` — the size LERP stays continuous. A continuous fade defeated `material_hash` dedup (~97%→~1 material/particle). Regression = color read back off the raw `t` fraction, which reinflates per-particle material churn.
**Output**: `/tmp/audit/renderer/dim_7.md`

#### Dimension 8: Denoiser & composite
**Entry points**: `crates/renderer/src/vulkan/svgf.rs`, `composite.rs`, `crates/renderer/shaders/svgf_temporal.comp`, `composite.frag`.
**Severity floor**: SVGF using wrong motion vectors = HIGH; ghosting / wrong tone-map order = MEDIUM.
**Checklist**:
- SVGF history ping-pong (read prev, write current); reprojection motion vectors match the vertex-shader output; mesh-ID disocclusion rejection prevents ghosting — for opaque draws the packed ID is now the **stable surface ID** (entity-based, survives per-frame draw reordering, #883f57cd) while alpha-blended draws still pack the current-frame instance index (caustic lookup needs live draw order, see Dim 11's mesh-ID bullet); blend α clamped, first-frame uses current (no garbage history); dispatch covers exactly the image (ceil division); per-frame history descriptor swap.
- Firefly rejection hoisted ahead of the `hasHistory` branch (regression guard, `48906670`) — verify the clamp applies on the no-history path too.
- Composite reassembly: direct + SVGF-denoised indirect + albedo (+ TAA-resolved HDR when TAA on), ACES tone-map applied **after** reassembly, bloom added **before** tone-map (Dim 16). Fog applied to direct only, not indirect. SSAO modulates indirect only.
- **Alpha-blend aux-MRT alpha lanes are no longer hardcoded to 1.0 (regression guard, #883f57cd).** `triangle.frag` now writes `auxiliaryAlpha = isAlphaBlend ? finalAlpha : 1.0` into both `outRawIndirect.a` and `outAlbedo.a` (effect/emissive early-outs and framebuffer-transmission/RT-terminus glass exits follow the same pattern) so the blend pipeline can preserve the opaque receiver's indirect/albedo when transmission is unresolved. Do not assume alpha≡1 on these MRTs for blended fragments when touching composite.
- Caustic accumulator (`R32_UINT`) sampled via `usampler2D`, divided by the fixed-point scale, added to **direct** (never the SVGF-denoised indirect — double-count guard, Dims 14/15).
- Composite output to swapchain (correct format/layout transition to `PRESENT_SRC_KHR`).
**Output**: `/tmp/audit/renderer/dim_8.md`

#### Dimension 9: GPU skinning compute + BLAS refit (M29)
**Entry points**: `crates/renderer/src/vulkan/skin_compute.rs`, `crates/renderer/shaders/skin_vertices.comp`, `skin_palette.comp`, `acceleration/blas_skinned.rs`, `byroredux/src/render/skinned.rs`, `byroredux/src/render/bone_palette_overflow_tests.rs`.
**Checklist**:
- `VERTEX_STRIDE_FLOATS = 25` (100 B/vertex) is defined in `crates/renderer/src/shader_constants_data.rs`, consumed by `skin_compute.rs` via `use crate::shader_constants::VERTEX_STRIDE_FLOATS` (NOT a hardcoded `25`), pinned against `size_of::<Vertex>()` by the assert in `skin_compute.rs` — drift corrupts every skinned vertex.
- `skin_palette.comp` (palette = `bone_world × bind_inverse`, GPU-side) pre-dispatches before `skin_vertices.comp`; both share a 64-wide workgroup; dispatch `(vertex_count + 63) / 64`.
- `SkinPushConstants` (vertex_offset/count, bone_offset) matches the GLSL push-constant struct, ≤ 128 B.
- Skinned output buffer usage flags include `STORAGE_BUFFER`, `ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR`, and `VERTEX_BUFFER` (#681).
- COMPUTE → AS-BUILD → FRAGMENT barrier scopes correct; refit (UPDATE mode) matches original BUILD geometry/vertex count; skinned BLAS pinned against LRU eviction while in flight; refit-count rebuild threshold per memory-budget.md. **Scratch-serialize barrier dst mask (regression guard, #1790).** `record_scratch_serialize_barrier` (`blas_skinned.rs`) uses `ACCELERATION_STRUCTURE_WRITE_KHR | ACCELERATION_STRUCTURE_READ_KHR`, not WRITE-only — an UPDATE-mode refit reads `srcAccelerationStructure`, and a first-sight frame's BUILD-then-refit in the same command buffer was an unmade-visible RAW hazard (confirmed by validation layer). A narrowed mask back to WRITE-only is the regression.
- Bone-palette overflow guard fires (`Once`-gated warn) at the cap — silent truncation past `MAX_TOTAL_BONES` was the M29 regression, pinned by `bone_palette_overflow_tests.rs`.
**Output**: `/tmp/audit/renderer/dim_9.md`

#### Dimension 10: Camera-relative render origin & f32 precision (#1495/#1496)
**Entry points**: `crates/renderer/src/vulkan/scene_buffer/gpu_types.rs` (`GpuCamera.render_origin`), `byroredux/src/render/camera.rs`, `crates/renderer/shaders/triangle.vert` / `triangle.frag`, `byroredux/src/cell_loader/references/mod.rs` (`RT_ABSOLUTE_PRECISION_CEILING`). Spec: shader-pipeline.md "Coordinate Spaces & Precision".
**Severity floor**: a path mixing the two conventions = HIGH (large-world precision corruption).
**Checklist**:
- **Two conventions, never mixed.** Raster runs render-origin-**relative** (`viewProj × worldPos_rel` keeps full f32 at large offsets); RT stays **absolute** (TLAS transforms, skinned BLAS, ray origins/lighting/fog reconstructed as `worldPos_rel + render_origin`).
- Rigid `GpuInstance.model` translation rebased on CPU; skinned path rebases blended bone-palette translation by `−render_origin` in `triangle.vert` (#1486).
- `triangle.vert` emits `fragWorldPosRel` (location 3) **relative**; `triangle.frag` reconstructs absolute at top of `main()` (#1496) so `dFdx/dFdy` consumers (flat-shading normal, `perturbNormal` TBN, `parallaxDisplaceUV`, rtLOD footprint) see small magnitudes. Verify no derivative consumer was moved back to the absolute varying.
- `RT_ABSOLUTE_PRECISION_CEILING = 2^20 = 1_048_576` — `references.rs` `debug_assert!`s loaded-cell max `|coord|` stays under it via `worldspace_extent_over_rt_ceiling` (unit-tested). Any new absolute-space shader consumer inherits this ceiling.
- DoF: degenerate `focus_dist` guarded (#1525); `GpuCamera` doc accuracy (#1526).
**Output**: `/tmp/audit/renderer/dim_10.md`

---

### MEDIUM tier — per-feature passes, shader correctness, visual

#### Dimension 11: Pipeline state & render pass / G-buffer
**Entry points**: `crates/renderer/src/vulkan/pipeline.rs`, `descriptors.rs`, `context/helpers.rs` (`create_render_pass`), `gbuffer.rs`.
**Severity floor**: G-buffer format mismatch (shader output vs attachment) = HIGH.
**Checklist** (formats/bindings are in shader-pipeline.md — audit the *match*, don't restate the table):
- Vertex input matches `crates/renderer/src/vertex.rs` (binding/location/format/offset); push-constant ranges match shader declarations; dynamic viewport/scissor (and dynamic `CULL_MODE` for water two-sided) set each frame.
- G-buffer pipeline writes all six color attachments; composite inputs match G-buffer + denoiser outputs; SSAO/cluster-cull compute descriptor layouts correct.
- Mesh-ID encoding: `R32_UINT`, bit 31 (`0x80000000`) = `ALPHA_BLEND_NO_HISTORY` (SVGF skip). Bits 0–30 changed *meaning* (not layout) under #883f57cd — `gbuffer.rs::MESH_ID_FORMAT` doc now reads "stable surface ID / alpha draw lookup": opaque draws pack `inst.surfaceId & 0x7FFFFFFF` (stable across per-frame depth-sort/batch reordering), alpha-blended draws still pack the current-frame instance index + 1 (caustic source lookup needs live draw order, not a stable identity). Encoded shader-side in `triangle.frag` (`meshIdBase = alphaBlendFrag ? sortedInstanceId : stableSurfaceId`, `outMeshID = meshIdBase | (alphaBlendFrag ? 0x80000000u : 0u)`); runtime overrun is a one-shot `warn!` + clamp in `draw_frame`/`upload_instances` (NOT a `debug_assert!` — moved off the assert to avoid leaking the in-flight cmd buffer on unwind, #956/#992).
- Render-pass load/store ops, layout transitions, subpass dependencies cover all stage/access masks; G-buffer images created `SAMPLED` (SVGF/composite read). **Flag barrier/dependency changes as needs-RenderDoc.**
- Pipeline cache: header pre-validated against the device before handoff; mismatch → warning + empty cache, no crash (`context/helpers.rs`, SAFE-11/#91).
**Output**: `/tmp/audit/renderer/dim_11.md`

#### Dimension 12: Command buffer recording
**Entry points**: `crates/renderer/src/vulkan/context/draw.rs`.
**Checklist**:
- Reset-before-record, begin/end balanced (command buffer + render pass), AS build recorded outside the render pass, SVGF/compute after RP end and before composite, composite last (then egui, then optional screenshot copy).
- Per-draw: depth bias for decals, pipeline/descriptor bind, push constants, indexed draw; batch coalescing groups draws by texture/descriptor.
- Counter independence (regression guards): `DrawCommand` input count vs post-batch GPU draw count are separate metrics, both surfaced (#1258); blend-pipeline cache-hit fast path exists at the per-draw bind site (#1259); off-frustum draws skip `GpuInstance.flags` assembly without dropping state on frustum-border visible draws (#1260); cell-loader REFR spawn attaches a per-entity `SceneFlags` from the NIF root `NiAVObject.flags` (`SceneFlags::from_nif(cached.root_flags)`) for parity with the loose-NIF loader (#1235).
- **Two-sided blend split no longer gates on `z_write` (regression guard, #883f57cd).** `needs_two_sided_blend_split` (`draw.rs`) is now `is_blend && b.two_sided` — the old `&& b.z_write` gate is gone, since FO4 BGEM glass is commonly `z_write: false` and was silently skipping the back/front split. Test renamed `splits_when_z_write_false` (was `does_not_split_when_z_write_false`) — a reintroduced `z_write` gate is the regression.
**Output**: `/tmp/audit/renderer/dim_12.md`

#### Dimension 13: TAA (M37.5)
**Entry points**: `crates/renderer/src/vulkan/taa.rs`, `crates/renderer/shaders/taa.comp`, Halton jitter assembly (`halton` fn + the `(jx, jy)` block in `draw_frame`) in `crates/renderer/src/vulkan/context/draw.rs`.
**Checklist**:
- Halton(2,3) jitter advances per frame (no seam), applied in NDC pixel units; un-jittered projection retained for motion-vector reconstruction.
- Per-frame-in-flight history slot (no aliasing); reprojection samples motion with linear/dilated filter (point causes edge wobble); 3×3 YCoCg neighborhood clamp on the history sample; mesh-ID disocclusion discards stale history; first-frame / `should_force_history_reset` forces α = 1.0 with no garbage read.
- Moving-pixel accumulation α floored under a parked camera (regression guard, #1497).
- History images in `GENERAL` (no per-frame UNDEFINED); `validate_set_layout` (`reflect.rs`) fires and matches Rust bindings; composite samples TAA output only when TAA on; disable path skips the dispatch entirely.
**Output**: `/tmp/audit/renderer/dim_13.md`

#### Dimension 14: Caustic splat (#321)
**Entry points**: `crates/renderer/src/vulkan/caustic.rs`, `crates/renderer/shaders/caustic_splat.comp`, composite consumption in `composite.frag`.
**Checklist**:
- Per-FIF `caustic_accum` (`R32_UINT`, `STORAGE|SAMPLED|TRANSFER_DST`); cleared via `vkCmdClearColorImage` before dispatch; HOST→COMPUTE + CLEAR→COMPUTE barriers before dispatch; COMPUTE→FRAGMENT before composite sample; stays in `GENERAL`.
- Accumulation via `imageAtomicAdd` on u32 fixed-point (no float race); fixed-point scale in `CausticParams` matches the composite divide.
- Source-pixel selection reads the material flag from `materials[material_id]` (post-R1), using `INSTANCE_FLAG_CAUSTIC_SOURCE` macro (not a hex literal, #1234). Output added to direct only.
- `caustic_splat.comp` "water-side caustic is the water shader's responsibility" comment matches the live `water.frag` impl, not a stub (Dim 15).
**Output**: `/tmp/audit/renderer/dim_14.md`

#### Dimension 15: Water (M38) + water-side caustics
**Entry points**: `crates/renderer/src/vulkan/water.rs`, `crates/renderer/src/vulkan/water_caustic.rs` (`WaterCausticAccum`), `crates/renderer/shaders/water.vert`/`water.frag`, `byroredux/src/cell_loader/water.rs`, `byroredux/src/systems/water.rs` (`submersion_system`). Shared water components live in `crates/core/src/ecs/components/water.rs`.
**Checklist**:
- WaterPlane spawned from interior/exterior cell water records (height/extent match); vertex displacement bounded, no NaN, no Z-fight at shoreline; Fresnel base ~0.02 (do NOT reuse glass IOR 1.5); RT reflect/refract (IOR ~1.33) miss → sky/backdrop with fog (not black/magenta).
- `submersion_system` flips `SubmersionState` at the water plane with no per-frame strobe; cell unload despawns water cleanly (no leaked BLAS vs post-unload TLAS); water doesn't cast opaque shadows; two-sided via dynamic `CULL_MODE`; sort key places water per `byroredux/src/render/sort_key_tests.rs` ordering; distinct `GpuMaterial` entry (no dedup collapse with glass).
- Procedural-noise precision bound marked for absolute-world UVs (regression guard, #1502).
- Water-side caustic synthesis (regression guards): `sun_direction` plumbed through `GpuCamera` and uploaded each frame (not stale-from-init); `WaterCausticAccum` lifecycle (per-FIF `R32_UINT`, GENERAL/TRANSFER_DST/GENERAL, reverse-order `destroy`) lives in `water_caustic.rs` while `water.rs` owns only the descriptor set/layout/pool; `water.frag` actually writes the accumulator via `imageAtomicAdd`; composite samples it into **direct** lighting (#1210 Phases A–E / #1255–#1257).
**Output**: `/tmp/audit/renderer/dim_15.md`

#### Dimension 16: Volumetrics (M55) & bloom (M58)
**Entry points**: `crates/renderer/src/vulkan/volumetrics.rs`, `bloom.rs`, `crates/renderer/shaders/volumetrics_inject.comp`, `volumetrics_integrate.comp`, `bloom_downsample.comp`, `bloom_upsample.comp`, composite consumption in `composite.frag`.
**Checklist** — volumetrics:
- Froxel grid 160×90×128 matches the inject `local_size`; dispatch covers exactly the grid; per-FIF buffer (~14 MiB/slot, no cross-frame WAR); inject does a single `TerminateOnFirstHit` shadow ray per froxel; integrate multiplies transmittance across the walk; HG `g` clamped to (−0.999, 0.999).
- Gate `VOLUMETRIC_OUTPUT_CONSUMED` (#928): if composite drops the sample, the dispatch must be skipped (not dispatched + ignored). Interior cells (no sun) produce neutral non-NaN output; resize rebinds both volumetric and composite descriptors (#905).

**Checklist** — bloom:
- 5 down-mips + 4 up-mips, `B10G11R11_UFLOAT` throughout (no R16G16B16A16 mid-chain); 4-tap bilinear down (weights sum 1.0), additive up (no [0,1] clamp); per-FIF mip chain (cross-frame WAR gated by fence — do NOT reintroduce the redundant pre-barriers removed in #931).
- Bloom added **before** ACES tone-map (HDR add); intensity constant in `composite.frag`; source is the un-tone-mapped HDR (NOT the TAA output — descriptor-binding regression pattern); disable path short-circuits both dispatch and composite addition.
**Output**: `/tmp/audit/renderer/dim_16.md`

#### Dimension 17: Disney BSDF / PBR gating (#1248–#1254) + soft shadows
**Entry points**: `crates/renderer/shaders/include/pbr.glsl` (Disney lobe fn *definitions*) + `crates/renderer/shaders/include/lighting.glsl` (gate + call sites; `triangle.frag` `#include`s both), `crates/renderer/src/vulkan/material.rs` (Disney preset constructors), `byroredux/src/render/sky.rs` (`sun_angular_radius`).
**Checklist** — Disney (symbol-anchored; flag bits live in `shader_constants_data.rs`, NOT hand-declared in the shader — verify, this was the #1357 migration):
- Gate is `MAT_FLAG_PBR_BSDF` (bit 5) only — grep `MAT_FLAG_PBR_BSDF` gate sites in `crates/renderer/shaders/include/lighting.glsl` + `include/pbr.glsl` and confirm each lights the Disney lobe (no FNV/FO3/Skyrim legacy path tripping it). FNV/FO3/Oblivion: zero materials set the flag → lobe unreachable. FO4/FO76/Starfield: BGSM is canonical → lobe is the expected path (regression is a BGSM falling back to Lambert).
- `dielectricF0FromIor(eta)` derives F0 from per-material IOR (not hardcoded 0.04), with input-domain clamp guarding `eta ≤ 0` (#1248/#1253). Per-material IOR now has explicit canonical sources (#41eedfe1): `Material::ior` defaults to `DEFAULT_DIELECTRIC_IOR = 1.5` (generic dielectric, F0≈0.04); glass classification instead applies `GLASS_SURFACE_BEHAVIOR` (`roughness 0.10, metalness 0.0, ior 1.45`) via `Material::apply_surface_behavior`, which must NOT overwrite authored `texture_path`/`normal_map`/`glow_map`/`uv_scale`/alpha (regression guard: `glass_behavior_preserves_authored_map_overlay`, `crates/core/src/ecs/components/material.rs`).
- `distributionGGXAniso(NdotH, HdotX, HdotY, ax, ay)` MUST degenerate exactly to isotropic GGX when `ax == ay` (legacy-compat contract, #1250); `deriveAxAy(roughness, anisotropic, …)` clamps `anisotropic` to [0,1] (half-axis convention per GLSL-PathTracer, NOT Disney-2012 `[-1,1]`) — verify no `sqrt(<0)` at `anisotropic = 1.0` (#1254).
- `disneyDiffuseSplit(...)` returns split lobes (Burley retro + sheen + Hanrahan-Krueger SSS); sheen is additive, NOT divided by π (#1249/#1252).
- `#1147 Phase 2b` siblings fire independently: `MAT_FLAG_TRANSLUCENCY` (bit 6) → SSS, modulated by `MAT_FLAG_TRANSLUCENCY_THICK_OBJECT` (bit 8) / `MAT_FLAG_TRANSLUCENCY_MIX_ALBEDO` (bit 9); `MAT_FLAG_MODEL_SPACE_NORMALS` (bit 7, set by #972) → model-space sampling. No spurious cross-activation.
- Disney preset constructors in `material.rs` match documented values (cross-ref GLSL-PathTracer per `reference_glsl_pathtracer.md`).

**Checklist** — soft shadows (M-LIGHT):
- `sun_angular_radius` ships in `GpuCamera`; shipping default `0.020` rad (sky-params assert caps < 0.10) — drift changes shadow softness globally.
- Single-tap stochastic cone sample around the sun, deterministic per-pixel-per-frame (no true RNG that breaks TAA history); `TerminateOnFirstHit`; TAA absorbs the noise (YCoCg clamp tolerance allows convergence).
- Interior fill (`radius < 0.0` → `isInteriorFill`) bypasses the cone sample; disocclusion single-sample fallback is not black.
**Output**: `/tmp/audit/renderer/dim_17.md`

#### Dimension 18: Sky / weather / exterior lighting (M33/M34)
**Entry points**: `byroredux/src/systems/weather.rs`, `byroredux/src/render/sky.rs`, `crates/plugin/src/esm/records/weather.rs`, `crates/renderer/shaders/triangle.frag` (sky gradient + cloud + fog). See also `/audit-exal`.
**Checklist**:
- `weather_system` advances game time monotonically; sun arc from CLMT TNAM hours (not hardcoded); TOD color easing matches legacy; weather fade blends AFTER the TOD lookup (`WeatherTransitionRes`); all 4 cloud layers active with world-XY parallax scaled by TOD wind.
- Sky gradient (zenith→horizon) from active TOD palette in the non-RT miss-fill, consistent with the GI miss "sky fill" (Dim 2); fog applied to direct only (Dim 8); interior fill at 0.6× ambient with `radius = −1` (unshadowed), gating RT shadow on `!isInteriorFill` (symbol-anchored, #1200).
- Disabled-WTHR fallback is neutral (no NaN / pitch-black); cell transition does not strobe TOD (palette is per-worldspace + global clock, not per-cell).
**Output**: `/tmp/audit/renderer/dim_18.md`

#### Dimension 19: Tangent-space & normal maps (M-NORMALS)
**Entry points**: `crates/nif/src/import/mesh/tangent.rs`, `crates/nif/src/import/mesh/bs_tri_shape.rs`, `crates/nif/src/blocks/tri_shape/bs_tri_shape.rs` (`VF_TANGENTS`), `crates/renderer/shaders/include/material_sampling.glsl` (`perturbNormal`).
**Checklist**:
- Oblivion/FO3/FNV: per-vertex tangents from `NiBinaryExtraData` "Tangent space …" — Bethesda's "tangent" is `∂P/∂V` and "bitangent" is `∂P/∂U` (the `CalcTangentSpace` swap). The decoder must read the **bitangent half** into `Vertex.tangent.xyz` and derive the sign from the tangent half (handedness regression #786). 
- FO4+ BSTriShape inline tangents when `VF_TANGENTS | VF_NORMALS` set (packed-vertex loop) — distinct from Skyrim, not gated on the wrong BSVER (#795/#796).
- Synthesized fallback (`synthesize_tangents`) produces unit-length tangents + consistent signs when the blob is missing/malformed.
- Sign convention `B = bitangent_sign * cross(N, T)` reconstructed from `Vertex.tangent.w`, consistent across all three import paths; Z-up→Y-up conversion applied to tangent xyz in lockstep with the normal (no path converting N but not T).
- `perturbNormal` default-on (#787/#788); `DBG_BYPASS_NORMAL_MAP` (0x10) runtime opt-out still recognized; the 13-bit `DBG_*` catalog pinned in lockstep (Dim 3).
- "Chrome posterized walls" is the magenta-checker placeholder × a correctly-loaded normal map — per `feedback_chrome_means_missing_textures.md`, run `tex.missing` before recommending any tangent-space fix.
**Output**: `/tmp/audit/renderer/dim_19.md`

#### Dimension 20: Debug overlay & GPU telemetry
**Entry points**: `crates/renderer/src/vulkan/egui_pass.rs` (`EguiPass`), `crates/renderer/src/vulkan/gpu_timers.rs` (`GpuPerFrameTimers`), `crates/debug-ui/`, wired in `context/draw.rs` + `context/mod.rs`.
**Checklist**:
- egui pass uses `loadOp = LOAD` + `initialLayout = PRESENT_SRC_KHR` and is recorded after composite; supplies its own incoming dependency (Dim 4, #1433); framebuffers recreated on resize; `Option<EguiPass>` taken + `destroy()`d before device teardown; disabled (`= None`) path skips the dispatch with no layout drift.
- GPU timers: one `VkQueryPool` per FIF slot; `cmd_reset_query_pool` before re-recording brackets; results read `MAX_FRAMES_IN_FLIGHT` behind; driver-absent (`timestamp_supported == false`) → `new()` returns `Ok(None)`, no unwrap in the draw path; skipped passes omit their bracket (no bogus interval).
- `dispatches_skipped` is a **skin-coverage** counter (`skin_compute.rs`, incremented in `draw.rs` when the bone palette is unchanged), surfaced via `mem.stats`/console — NOT a `GpuPerFrameTimers` field. Issued dispatches = total − skipped (#1194).
**Output**: `/tmp/audit/renderer/dim_20.md`

#### Dimension 21: Cornell-box RT harness
**Entry points**: `byroredux/src/cornell.rs` (`setup_cornell_scene`, `--cornell` flag), `mat.*` console commands in `byroredux/src/commands/scene.rs`.
**Checklist**:
- The harness is a self-contained RT material/lighting reference scene (no on-disk game data) — verify it still builds a valid TLAS and renders without the asset pipeline, so it stays usable for bisecting glass/GI/caustic regressions (the Session 47 arc).
- `mat.*` live commands drive material params at runtime for A/B verification — confirm they round-trip into `GpuMaterial` via the same `MaterialTable` path as game content (no Cornell-only material shortcut that would invalidate it as a reference).
- Known confound (per memory): metalness-vs-lighting and the glass-stipple / IGN refraction jitter on opaque glass are open observations, not harness bugs — don't re-report them as new.
**Output**: `/tmp/audit/renderer/dim_21.md`

#### Dimension 22: Light animation canonical translation (flicker/pulse)
**Entry points**: `byroredux/src/systems/light_anim.rs` (`canonical_light_animation_flags`), `crates/core/src/ecs/components/light.rs` (`LightFlicker.animation_flags`), attach site `byroredux/src/cell_loader/references/mod.rs`.
**Severity floor**: wrong per-game flag decode = MEDIUM (visual-only; feeds `LightSource` intensity into the light buffer, no crash/corruption risk).
**Checklist**:
- `canonical_light_animation_flags(game, source_flags)` is the single per-game→shared-behavior boundary (mirrors the NIFAL translate pattern, Dim 6) — FO4 masks raw LIGH flags to `FLICKER | PULSE` only, dropping raw bit `0x400` (FO4's Shadow-Spotlight flag, NOT a slow-pulse animation); other games mask to the full `SHARED_LIGHT_ANIMATION_MASK` (`FLICKER | FLICKER_SLOW | PULSE | PULSE_SLOW`). Regression guards: `fallout4_shadow_spotlight_is_not_slow_pulse`, `fallout4_real_flicker_and_pulse_map_to_shared_behavior`.
- `LightFlicker.animation_flags` holds the translated value; `animate_lights_system` / `flicker_intensity` must read `animation_flags`, never the raw `LightSource.flags` — a caller reading raw flags reintroduces the FO4 shadow-spotlight-flickers-like-a-torch bug.
**Output**: `/tmp/audit/renderer/dim_22.md`

## Phase 3: Merge

1. Read all `/tmp/audit/renderer/dim_*.md`.
2. Combine into `docs/audits/AUDIT_RENDERER_<TODAY>.md`:
   - **Executive Summary** — findings by severity, pipeline areas affected.
   - **RT Pipeline Assessment** — BLAS/TLAS + SSBO indexing + ray-query safety + denoiser stability.
   - **GPU-Struct & Memory Assessment** — layout pins, leaks, lifecycle/teardown.
   - **Findings** — grouped by severity (CRITICAL first), deduplicated.
   - **Prioritized Fix Order** — correctness → safety → optimization.
   - **Needs-RenderDoc** — sync/barrier findings deferred for capture-based verification.
3. Remove cross-dimension duplicates.

## Phase 4: Cleanup

1. `rm -rf /tmp/audit/renderer`.
2. Inform the user the report is ready.
3. Suggest: `/audit-publish docs/audits/AUDIT_RENDERER_<TODAY>.md`.
