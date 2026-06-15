# Renderer Audit — 2026-06-14 (comprehensive sweep)

**Scope**: Full `/audit-renderer` run, all 23 dimensions, **deep** (data-flow
trace + invariant validation). Part of a `comprehensive` audit-suite sweep.

This report **supersedes and folds in** two focused single-dimension runs done
earlier the same day:

- **Dim 19 (Tangent-Space / M-NORMALS)** — its sole finding (**REN-D16-01**,
  MEDIUM, inverted inline bitangent sign) is carried verbatim below under MEDIUM.
- **Dim 13 (TAA / M37.5)** — `docs/audits/AUDIT_RENDERER_2026-06-14_DIM11.md`
  (mislabeled filename; content is TAA). Zero correctness findings; 2 LOW
  (DoF degenerate-input guard gap + `GpuCamera` doc-rot, the latter pinned
  precisely below as REN-D3-DOC-01). Not re-derived here; that report stands.

This sweep covers the remaining 21 dimensions. Each finding was re-derived
against the **current** code path and an attempt was made to disprove it before
inclusion; the majority of prior-audit candidates were disproved as stale.

---

## Executive Summary

| Severity | Count |
|----------|-------|
| CRITICAL | 0 |
| HIGH     | 1 |
| MEDIUM   | 1 |
| LOW      | 11 |
| **Total**| **13** |

The RT core (BLAS/TLAS geometry + addresses, `instance_custom_index` ↔ SSBO
contract, GPU-struct byte layouts, sync/barrier chains, memory lifecycle/teardown,
NIFAL material boundary, denoiser, skinning, camera-relative precision) is in a
**mature, well-pinned state** — Dims 1, 3, 4, 5, 6, 7, 8, 9, 10 produced zero
findings above LOW, with each high-risk invariant guarded by a unit test or
const-assert.

The **HIGH** is a robustness/spec gap in the **water path**: `water.frag` fires
RT ray queries with no RT-availability gate, and the water draw / pipeline
creation are not gated on `device_caps.ray_query_supported`. On a GPU lacking
`VK_KHR_ray_query`, the binding-2 (TLAS) slot is intentionally dropped from the
scene descriptor layout, so a water cell drives a draw whose shader statically
references an absent binding while carrying the `RayQueryKHR` capability with the
`rayQuery` device feature disabled.

The **MEDIUM** (carried from the focused Dim-19 run) is an inverted bitangent
sign on the two inline packed-vertex tangent paths (Skyrim SE / FO4 / FO76
BSTriShape + SSE skin-reconstruction).

The 11 LOW findings are dominated by **documentation drift** in the two
authoritative reference docs (`shader-pipeline.md`, `memory-budget.md`), which
the skill itself partially inherited — most notably the G-buffer is actually
**7 color attachments** (not 6) and the HDR attachment is **RGBA16F** (not the
documented packed `B10G11R11`).

---

## RT Pipeline Assessment

- **BLAS/TLAS (Dim 1)**: geometry format `R32G32B32_SFLOAT` @ offset 0 / `UINT32` /
  `OPAQUE` correct in both static + skinned paths; `instance_custom_index =
  Packed24_8(ssbo_idx, 0xFF)` 1:1 with the instance SSBO and compile-time-asserted
  under the 24-bit ceiling (`MAX_INSTANCES = 0x40000`); TLAS UPDATE keys on
  `last_blas_addresses` with VUID-03708/03667 guards; transform 3×4 row-major;
  `built_flags` refit assert intact. 68/68 acceleration tests pass. Only gap: no
  value-pin test for `STATIC_BLAS_FLAGS` (LOW).
- **SSBO / ray queries (Dim 2)**: all 6 `triangle.frag` ray-query sites are gated
  behind `rtEnabled` (`sceneFlags.x > 0.5`); custom-index SSBO indexing, shadow/
  reflection/GI bias + tMin, Frisvad bases (#820/#574), window-portal demote
  (#789), glass-budget overshoot-by-design (#1438), interior-miss cell-ambient
  (#1125), and deterministic IGN seeding all verified clean. The **HIGH** is the
  un-gated `water.frag` path (below).
- **GPU-struct layout (Dim 3)**: `GpuInstance` 112 B, `GpuCamera` 336 B,
  `GpuMaterial` 300 B — all internally consistent, per-field offsets pinned, all
  fields scalar (no `[f32;3]`), 5-shader `GpuInstance` mirror field-identical,
  flag constants single-sourced from `shader_constants_data.rs`. (Docs lag — LOW.)
- **Denoiser/composite (Dim 8)**: SVGF ping-pong + motion-vector match + mesh-ID
  disocclusion + firefly hoist (48906670) correct; composite reassembly order
  (bloom/vol pre-ACES, ACES after, caustic into direct only) correct.
- **Skinning (Dim 9)**: `VERTEX_STRIDE_FLOATS` single-sourced + pinned;
  COMPUTE→AS-BUILD→FRAGMENT chain correct; refit count/flag validation; bone-palette
  overflow guard fires. (Stale open #1387 should be closed — LOW.)
- **Camera-relative precision (Dim 10)**: the raster-relative / RT-absolute split
  is clean — TLAS reads absolute `model_matrix`, raster `GpuInstance.model` is
  CPU-rebased by `-render_origin`, `triangle.frag` reconstructs absolute at top of
  `main()`, derivative consumers use the relative varying, `RT_ABSOLUTE_PRECISION_CEILING`
  asserted. No convention mixing.

## GPU-Struct & Memory Assessment

- **Memory/lifecycle (Dim 5)**: memory-type correctness, destroy→allocator→device
  ordering, `AllocatorResource` panic-safe ECS-ordering (#1406/#1483), TLAS-resize
  `device_wait_idle` (#1390), pool caps, deferred-destroy depth, and reverse-order
  teardown all verified. No per-frame leaks. (1 LOW existing #1427 + 1 LOW doc-rot.)
- **Pipeline/render-pass (Dim 11)**: vertex input ↔ `vertex.rs`, push-constant
  ranges, dynamic viewport/scissor/cull, mesh-ID encode (warn+clamp not assert,
  #956/#992), G-buffer SAMPLED usage, pipeline-cache header pre-validation all
  correct. (3 LOW: 2 doc-rot + 1 test-coverage.)

## Findings

### HIGH

#### REN-D2-NEW-01: water.frag fires RT ray queries with no RT-capability gate; on non-RT hardware binding 2 (TLAS) is absent from the scene set layout
- **Severity**: HIGH
- **Dimension**: Ray Queries (descriptor/binding plumbing)
- **Location**: `crates/renderer/shaders/water.frag` (`traceWaterRay`, `foamShoreline`, `main` sun-shadow + floor rays — all `rayQueryInitializeEXT` against `topLevelAS` at set=1 binding=2); `crates/renderer/src/vulkan/water.rs` (`WaterPipeline::new`); `crates/renderer/src/vulkan/scene_buffer/buffers.rs` (`build_scene_descriptor_bindings`); `byroredux/src/render/water.rs` (`reemit_water_planes`); `crates/renderer/src/vulkan/context/draw.rs` (water draw block, `if !water_commands.is_empty()`)
- **Status**: NEW (no open/closed issue tracks a water non-RT gate; grep of `crates/renderer` + `byroredux` for `ray_query_supported` in any water file returns nothing; issues.json has no match)
- **Description**: `triangle.frag` guards *every* ray query behind `rtEnabled = sceneFlags.x > 0.5`, uploaded as `0.0` whenever `device_caps.ray_query_supported == false`. That is exactly why `build_scene_descriptor_bindings(rt_enabled=false)` is allowed to **omit binding 2 entirely** and `validate_set_layout` lists `[2]` in `optional_bindings` for the no-RT case — triangle.frag's binding-2 use is *dynamically* unreachable when RT is off. `water.frag` has **no equivalent gate**: `traceWaterRay`/`foamShoreline`/the sun-shadow ray/the floor ray all run unconditionally in `main()` (water.frag declares `sceneFlags` but never reads `sceneFlags.x`). Meanwhile (a) `WaterPipeline::new` is created unconditionally (not gated on `ray_query_supported`); (b) `reemit_water_planes` emits `WaterDrawCommand`s for every `WaterPlane` with no RT gate; (c) the water draw block is gated only on `!water_commands.is_empty()` && `self.water.is_some()`. So on a Vulkan GPU lacking `VK_KHR_ray_query`/`acceleration_structure`, a water cell drives a draw whose shader statically uses set=1 binding=2 (TLAS) absent from the bound layout, with SPIR-V carrying the `RayQueryKHR` capability while the `rayQuery` device feature was left disabled.
- **Evidence**:
  - `buffers.rs` (`build_scene_descriptor_bindings`): `if rt_enabled { bindings.push(...binding(2)...ACCELERATION_STRUCTURE_KHR...) }` — binding 2 omitted when RT off; `let optional_bindings = if rt_enabled { &[] } else { &[2] };` with the comment "the shader still declares it because `rayQuery` calls are guarded by a uniform flag at runtime" (holds for triangle.frag, NOT water.frag).
  - `context/mod.rs`: `accel_manager = if device_caps.ray_query_supported { ... } else { None }`; with `None`, `write_tlas` is never called and `tlas_written[frame]` stays `false` for the session, so binding 2 is never written.
  - `context/mod.rs` (`WaterPipeline::new` call): water pipeline built regardless of RT support (logs + `None` only on shader/pipeline *error*).
  - `water.frag`: `layout(set = 1, binding = 2) uniform accelerationStructureEXT topLevelAS;` used with no `sceneFlags.x`/`rtEnabled` guard. (Contrast: `caustic_splat.comp` has `if (sceneFlags.x < 0.5) return;`.)
  - `byroredux/src/render/water.rs` (`reemit_water_planes`) — no `ray_query_supported` check.
- **Impact**: On non-RT hardware the engine has no explicit guard preventing the water draw. Best case (most likely): pipeline creation fails on the `RayQueryKHR`-capability-without-feature mismatch → `water = None`, water silently never renders (graceful but undocumented). Worst case (driver-dependent, **needs RenderDoc / a non-RT device to pin**): the pipeline creates and a ray query executes against a binding-2 slot absent from the bound layout → validation error / undefined behaviour / device loss. Either way the "RT off ⇒ no ray queries run" contract that the whole `optional_bindings=[2]` design rests on is violated by the water path. NOT a concern on RT-capable hardware (binding 2 is always a valid, possibly-empty TLAS there).
- **Related**: REN-D15-NEW-01 (same root cause; also notes the RT-hardware first-per-slot stale-TLAS sub-case).
- **Suggested Fix**: Gate the water subsystem on RT support to match triangle.frag's contract. Cheapest: skip the water draw block in `draw.rs` when `!self.device_caps.ray_query_supported` (or `!self.scene_buffers.tlas_written[frame]`), and/or skip `WaterPipeline::new` on non-RT devices (mirroring `accel_manager`/`skin_compute`/`skin_palette`). Belt-and-suspenders: add water.vert/water.frag to a `validate_set_layout` call with `optional_bindings=[2]` so a layout/shader drift is caught at startup. (Pipeline-creation-failure-mode part is invisible to `cargo test` — verify on a non-RT device or with RenderDoc.)

### MEDIUM

#### REN-D16-01: BSTriShape inline + SSE-reconstruction tangent paths use an inverted bitangent-sign convention
- **Severity**: MEDIUM
- **Dimension**: Tangent-Space & Normal Maps (M-NORMALS)
- **Location**: `crates/nif/src/blocks/tri_shape/bs_tri_shape.rs` (inline packed-vertex tangent decode) and `crates/nif/src/import/mesh/sse_recon.rs` (SSE skin reconstruction tangent path)
- **Status**: NEW (carried from the focused Dim-19 run earlier 2026-06-14; orchestrator-re-derived)
- **Description**: The renderer fixes one global convention: `Vertex.tangent.xyz = ∂P/∂U`, and the shader reconstructs `B = vertexTangent.w * cross(N, T)` (`triangle.frag`). For `B` to land on the true `∂P/∂V`, the authored `w` must equal `sign(dot(∂P/∂V, cross(N, ∂P/∂U)))` — the formula used by `extract_tangents_from_extra_data` (`tangent.rs`) + `synthesize_tangents`, pinned by `tangent_convention_tests.rs` (RH winding ⇒ +1). The two inline packed-vertex paths compute the triple product with the two operand vectors **swapped** (`cross(N, t_xyz)` where `t_xyz` = on-disk `∂P/∂V`, dotted with the bitangent triplet `∂P/∂U`). Because the scalar triple product is antisymmetric under swapping two vectors, the inline paths emit the **opposite** sign for the same geometry; the in-code comments claim parity with `extract_tangents_from_extra_data` but T and B are interchanged.
- **Evidence**: Numeric reproduction on the textbook RH fixture (N=+Z, ∂P/∂U=+X, ∂P/∂V=+Y) — the case `tangent_convention_tests.rs` pins at +1: authored/synth `dot(∂P/∂V, cross(N, ∂P/∂U)) = +1.0` (B = +∂P/∂V, correct) vs inline `dot(∂P/∂U, cross(N, ∂P/∂V)) = −1.0` (B = −∂P/∂V, inverted). No unit test exercises the inline/SSE sign — the convention tests only call `synthesize_tangents{,_yup}`. The inline `w` reaches the shader untouched and both arrays land non-empty in `Vertex.tangent`, so the shader takes its Path-1 branch.
- **Impact**: Reconstructed bitangent B is negated for every BSTriShape mesh shipping inline `VF_TANGENTS` (the common Skyrim SE / FO4 / FO76 case) and every SSE skin-reconstructed body/creature. The tangent-space normal map's V (green) channel reads with flipped handedness → directional carved normals read as a consistent "inside-out groove". It also makes handedness **disagree between sibling paths** (inline-tangent mesh inverted, synthesized-fallback mesh correct, on identical content). Rated MEDIUM (consistent global flip on a subtle channel, not corruption/crash) — arguably HIGH for the affected games given it is their primary geometry path.
- **Suggested Fix**: Swap the operands so the inline/SSE formula matches the pinned convention — `sign(dot(t_xyz /*∂P/∂V*/, cross(N, [bx,by,bz] /*∂P/∂U*/)))` (or negate the existing `dot_b_cross`). Add an inline-path case to `tangent_convention_tests.rs` (RH fixture ⇒ +1) so both packed paths are pinned to the same canonical sign as `synthesize_tangents`.

### LOW

#### REN-D11-01: shader-pipeline.md G-buffer table omits the 7th color attachment (ReSTIR-DI reservoir)
- **Severity**: LOW
- **Dimension**: Pipeline/RenderPass
- **Location**: `docs/engine/shader-pipeline.md` (G-Buffer Layout table) vs `crates/renderer/src/vulkan/context/helpers.rs` (`create_render_pass`), `crates/renderer/src/vulkan/gbuffer.rs` (`RESERVOIR_FORMAT`)
- **Status**: NEW
- **Description**: The doc says "Six colour attachments + depth"; the live main render pass declares **seven** color attachments — attachment 6 is the ReSTIR-DI reservoir (`RESERVOIR_FORMAT = R32G32B32A32_UINT`), with depth at attachment 7. The skill's checklist inherited the stale "six color attachments" figure from this doc.
- **Evidence**: `create_render_pass` builds `color_refs = [0..=6]` (7), `depth_ref { attachment: 7 }`, logs "Render pass created (7 color + depth)"; `triangle.frag` writes `layout(location = 6) out uvec4 outReservoir;`; all opaque/blend/water/UI pipelines declare 7 color-blend entries (reservoir slot `blend_enable(false)`).
- **Impact**: Doc-only; code internally consistent. A contributor under-sizing a new pipeline's blend-attachment array to 6 would trip VUID-vkCmdDrawIndexed-blendEnable-04727 (OOB read sees blendEnable=TRUE on the integer reservoir attachment).
- **Suggested Fix**: Add the reservoir row + change "Six" → "Seven" in `shader-pipeline.md`.

#### REN-D11-02: shader-pipeline.md lists HDR color attachment as B10G11R11_UFLOAT_PACK32; live format is R16G16B16A16_SFLOAT (alpha is load-bearing)
- **Severity**: LOW
- **Dimension**: Pipeline/RenderPass
- **Location**: `docs/engine/shader-pipeline.md` (G-Buffer "HDR colour" row) vs `crates/renderer/src/vulkan/composite.rs` (`HDR_FORMAT`)
- **Status**: NEW
- **Description**: The doc claims the HDR colour attachment is `B10G11R11_UFLOAT_PACK32` (no alpha). The actual format (`GBufferFormats.color_format = HDR_FORMAT`) is `R16G16B16A16_SFLOAT`; the blend + water pipelines depend on that alpha channel for SRC_ALPHA blending.
- **Evidence**: `composite.rs`: `pub const HDR_FORMAT = R16G16B16A16_SFLOAT`; `helpers.rs` inline comment "0 — HDR color (RGBA16F)"; shaders write `outColor = vec4(..., alpha)`; blend factors `src_alpha`/`one_minus_src_alpha`.
- **Impact**: Doc-only but actively misleading: a contributor "fixing" the attachment to the documented packed (alpha-less) format would silently break alpha-blended/water output.
- **Suggested Fix**: Correct the HDR row to `R16G16B16A16_SFLOAT` and note the alpha feeds blend/water.

#### REN-D11-03: No automated test pins fragment-output count to render-pass color-attachment count (7-way match hand-maintained across 4 sites)
- **Severity**: LOW
- **Dimension**: Pipeline/RenderPass
- **Location**: `crates/renderer/src/vulkan/reflect.rs` (`validate_set_layout` scope), `pipeline.rs` (blend arrays), `water.rs` (`build_pipeline`), `context/helpers.rs` (`create_render_pass`)
- **Status**: NEW
- **Description**: SPIR-V reflection validates descriptor-set layouts but explicitly excludes fragment color **outputs**; nothing asserts `attachment_count == 7` or `frag_output_count == attachment_count`. The "7" is hand-replicated across the render pass + 4 pipeline factories.
- **Evidence**: `reflect.rs` doc excludes outputs; each pipeline factory hardcodes a 7-element attachment array; no `#[test]` pins the count.
- **Impact**: A future G-buffer attachment add/remove must be mirrored across 5 sites by hand; a miss is `cargo test`-invisible (validation error / water OOB blend-state read at runtime). Tech-debt, not a current defect.
- **Suggested Fix**: Add a shared `MAIN_PASS_COLOR_ATTACHMENT_COUNT = 7` const + a unit test asserting each pipeline's blend-array length equals it (compile-time/array-length only — no barrier state, no RenderDoc).

#### REN-D3-DOC-01: shader-pipeline.md + memory-budget.md say GpuCamera is 320 B; code is 336 B
- **Severity**: LOW
- **Dimension**: GPU-Struct Layout
- **Location**: `docs/engine/shader-pipeline.md` (GpuCamera section + descriptor-set table), `docs/engine/memory-budget.md` ("Camera UBO" row) vs `crates/renderer/src/vulkan/scene_buffer/gpu_types.rs` (`GpuCamera`)
- **Status**: NEW (the prior 06-14 TAA report flagged "GpuCamera doc-rot" generically; this pins the two specific stale doc sites)
- **Description**: `GpuCamera` grew 320→336 B with the `render_origin: [f32;4]` field (#markarth-precision/#1492). Code is consistent (`gpu_camera_is_336_bytes` pins 336); two docs still say 320.
- **Evidence**: `gpu_camera_is_336_bytes`; `shader-pipeline.md` "320 bytes" + table "(320 B)"; `memory-budget.md` "320 B".
- **Impact**: Doc-only. A reader sizing a UBO from the doc under-allocates by 16 B.
- **Suggested Fix**: Update both docs to 336 B and add the `render_origin` row to the GpuCamera field table.

#### REN-D3-DOC-02: material.rs hash/collision doc comments say "50 scalar fields"; GpuMaterial has 75
- **Severity**: LOW
- **Dimension**: GPU-Struct Layout
- **Location**: `crates/renderer/src/vulkan/material.rs` (`hash_gpu_material_fields` doc; `intern_by_hash` collision-policy doc)
- **Status**: NEW
- **Description**: `GpuMaterial` is 300 B = 75 scalar slots; the hash walk writes exactly 75 `h.write_u32` (full coverage, verified by diffing struct field names against hash-walk names — zero missing/extra/dup). Two doc comments still say "50 live scalar fields", predating the ior/subsurface/sheen/sheen_tint/anisotropic (#1248-#1250) + translucency (#1147) additions.
- **Evidence**: `awk` count of `h.write_u32` = 75; `pub <field>:` count = 75; `comm` diff empty both directions.
- **Impact**: Doc-only; hash is correct/complete. A reader reasoning about collision probability from "50" has a wrong premise.
- **Suggested Fix**: Change both "50" → "every scalar field" (drop the hardcoded count).

#### REN-D5-01: EguiPass::destroy() does not flush pending_free before Renderer drop
- **Severity**: LOW
- **Dimension**: Memory/Lifecycle
- **Location**: `crates/renderer/src/vulkan/egui_pass.rs` (`EguiPass::destroy`, field `pending_free`)
- **Status**: Existing: #1427
- **Description**: `EguiPass::destroy()` drains framebuffers + destroys the render pass but never calls `free_textures(&pending_free)`, so the last frame's `textures_delta.free` ids skip the per-id `free_descriptor_sets` accounting step.
- **Evidence**: `destroy` only loops framebuffers + `destroy_render_pass`; `pending_free` untouched. **CONFIRMED NOT A VRAM LEAK**: the vendored `egui-ash-renderer` `impl Drop for Renderer` drains `managed_textures` + destroys the whole descriptor pool (implicitly freeing every set); those textures were never removed from `managed_textures`, so Drop reclaims them in bulk.
- **Impact**: Cosmetic — descriptor-pool free-count internally inconsistent at teardown; no leaked memory/image, no per-frame growth (steady-state `dispatch()` flushes every frame).
- **Suggested Fix** (per #1427): in `destroy`, `let drained = std::mem::take(&mut self.pending_free); let _ = self.renderer.free_textures(&drained);`.

#### REN-D5-DOC-01: memory-budget.md says warn fires at 75% of any heap; code is 80% of smallest DEVICE_LOCAL heap on allocated bytes
- **Severity**: LOW
- **Dimension**: Memory/Lifecycle (doc-vs-code)
- **Location**: `docs/engine/memory-budget.md` (VRAM-budget section) vs `crates/renderer/src/vulkan/allocator.rs` (`warn_threshold_bytes`, `log_memory_usage`)
- **Status**: NEW
- **Description**: The doc says "A warning fires if heap utilisation exceeds 75% of any heap type." Actual: **80%** (`(heap/5)*4`) of the **smallest single DEVICE_LOCAL heap**, compared against `total_allocated_bytes` only.
- **Evidence**: `warn_threshold_bytes` returns `(heap/5)*4` (heap = smallest device-local, 2 GB fallback); `log_memory_usage` warns when `total_allocated_bytes > threshold`; `warn_threshold_falls_back_when_heap_missing` pins the 80% math.
- **Impact**: Doc-only; could mislead VRAM-pressure triage (warn is later than documented, single-heap-scoped, on allocated not reserved bytes).
- **Suggested Fix**: Update the doc to the 80%/smallest-DEVICE_LOCAL-heap/allocated-bytes wording.

#### AS-D1-01: STATIC_BLAS_FLAGS has no value-pinning test (siblings do)
- **Severity**: LOW
- **Dimension**: AS Correctness
- **Location**: `crates/renderer/src/vulkan/acceleration/constants.rs` (`STATIC_BLAS_FLAGS`), `acceleration/tests.rs`
- **Status**: NEW
- **Description**: `UPDATABLE_AS_FLAGS` and `SKINNED_BLAS_FLAGS` both have value-pin tests; `STATIC_BLAS_FLAGS` (`FAST_TRACE | ALLOW_COMPACTION`) does not. Test-coverage symmetry gap, not a live bug — the constant is shared across all static-BLAS call sites so the VUID lockstep it protects can't regress; only the perf/compaction intent could silently drift.
- **Evidence**: `grep` for `static_blas_flags` test → none; the two sibling pin tests present in `tests.rs`.
- **Impact**: None at runtime; a future flag-set change to `STATIC_BLAS_FLAGS` would be unguarded by a test.
- **Suggested Fix**: Add `static_blas_flags_is_fast_trace_allow_compaction` mirroring the two sibling tests.

#### DIM9-DEDUP-01: Open issue #1387 (RT-04) is stale — VERTEX_BUFFER tracking comment now present
- **Severity**: LOW (issue-hygiene)
- **Dimension**: Skinning
- **Location**: `crates/renderer/src/vulkan/skin_compute.rs` (`SkinComputePipeline::create_slot`)
- **Status**: Existing: #1387 (should be CLOSED — verified resolved in live code)
- **Description**: #1387 asks for a tracking comment at the skin-output-buffer creation site explaining the deliberate VERTEX_BUFFER omission. That comment now exists (added by commit `b99ae91e`, the #681 fix). The remediation #1387 requested has landed.
- **Evidence**: `git log -L` shows `b99ae91e` removed the flag AND added the comment; current flags `STORAGE_BUFFER | SHADER_DEVICE_ADDRESS | ACCELERATION_STRUCTURE_BUILD_INPUT_READ_ONLY_KHR` match resolved #681. NOTE: the skill's Dim-9 checklist item "usage flags include … VERTEX_BUFFER (#681)" has a **stale premise** — #681 *removed* VERTEX_BUFFER; it must stay absent until the M29.3 raster-reads-skinned-output path ships.
- **Impact**: None at runtime; a stale OPEN issue inflates the backlog and propagates a wrong "VERTEX_BUFFER should be present" premise.
- **Suggested Fix**: Close #1387 with a pointer to `b99ae91e`. No code change.

#### REN-D16-2026-06-14-01: volumetrics.rs froxel-size doc-comment understates per-slot allocation by 2×
- **Severity**: LOW
- **Dimension**: Volumetrics
- **Location**: `crates/renderer/src/vulkan/volumetrics.rs` (`FROXEL_WIDTH/HEIGHT/DEPTH` doc-comment)
- **Status**: NEW
- **Description**: The doc-comment states "14.06 MiB per slot, ×2 frames-in-flight = 28.12 MiB total," but `new_inner` allocates **two** 3D volumes per FIF slot (`lighting_volumes` + `integrated_volumes`), so the real total is ~56 MiB. The pipeline's startup `log::info!` is correct ("2× MiB/slot inject + integrated"), making the constant comment internally inconsistent.
- **Evidence**: `new_inner` allocates both volume sets per slot; the info-log already accounts for 2×.
- **Impact**: Doc-only; allocation and logging are correct.
- **Suggested Fix**: Update the constant doc-comment to ~28 MiB/slot (2 volumes) → ~56 MiB total.

#### REN-D14-NEW-01: Combined caustic accumulator sum can wrap u32 before the float divide
- **Severity**: LOW
- **Dimension**: Caustics
- **Location**: `crates/renderer/shaders/composite.frag` (`main`, the `causticRaw + waterCausticRaw` term)
- **Status**: NEW (theoretical)
- **Description**: composite does `float(causticRaw + waterCausticRaw) / CAUSTIC_FIXED_SCALE`. Each per-pixel accumulator is clamped *per deposit* to `0xFFFFFFFFu / scale`, but the atomic *sum* across many deposits can climb toward the u32 ceiling on either accumulator; adding two large values wraps modulo 2^32 to a small number → near-black pixel where a bright caustic cusp should be. The `min(…, 16.0)` firefly cap runs AFTER the divide and cannot recover a wrapped value.
- **Evidence**: `caustic_splat.comp` bounds each `imageAtomicAdd` argument, not the running per-pixel total; `composite.frag` sums two accumulators before the divide + cap.
- **Impact**: Cosmetic flicker (occasional dark pixel) only at extreme caustic concentration with overlapping glass + water caustics. Not observed in shipping content (physical attenuation keeps values well below the ceiling).
- **Suggested Fix**: Promote to 64-bit before the add, or clamp each raw to a shared `CAP <= 0x7FFFFFFF` before summing.

#### REN-D15-NEW-01: water.frag RT rays + caustic splat not gated on sceneFlags.x (RT-availability / TLAS-written)
- **Severity**: LOW
- **Dimension**: Water
- **Location**: `crates/renderer/shaders/water.frag` (`main` — reflection, refraction, foamShoreline, the `#1256 / Phase D` caustic splat); draw-side gate absent in `context/draw.rs::draw_frame`
- **Status**: NEW — RELATED to REN-D2-NEW-01 (do not double-count)
- **Description**: Every ray-query path in water.frag runs unconditionally. Unlike `caustic_splat.comp` (`if (sceneFlags.x < 0.5) return;`), water.frag has no `sceneFlags.x` guard; the water-caustic splat is gated only on `sunDirection.w > 0.0`. Same root cause as the Dim-2 HIGH; the additional observation is that **even with RT hardware**, water.frag does not consult the per-frame TLAS-written bit, so on the first frame of a slot (before `write_tlas`) or a TLAS-failure frame it traces against an unwritten/stale TLAS — a readback cost the compute caustic path explicitly avoids.
- **Evidence**: `water.frag` declares `sceneFlags` but never checks `sceneFlags.x` before its `rayQueryInitializeEXT` calls; `caustic_splat.comp` has the `if (sceneFlags.x < 0.5) return;` early-out.
- **Impact**: Non-RT hardware covered by REN-D2-NEW-01. On RT hardware: wasted traversal against unwritten TLAS on the first per-slot frame / TLAS-failure frames; one-frame stale-geometry caustic deposit. Minor.
- **Suggested Fix**: Add `if (sceneFlags.x < 0.5)` early-outs around the water RT paths (mirroring `caustic_splat.comp`) AND gate the CPU water-draw loop on `ray_query_supported` (the REN-D2-NEW-01 fix). Ship together.

---

## Prioritized Fix Order

1. **REN-D2-NEW-01 (HIGH, correctness/spec)** — gate the water subsystem on
   `ray_query_supported` (draw-loop skip + `WaterPipeline::new` skip) and add the
   `sceneFlags.x` early-outs from REN-D15-NEW-01 in the same change. Verify the
   non-RT pipeline-creation failure mode on a non-RT device / RenderDoc.
2. **REN-D16-01 (MEDIUM, correctness)** — swap/negate the inline + SSE
   bitangent-sign operands and add a pin test. Single-site logic fix on each of
   two functions plus one new test; no API/struct change.
3. **Issue hygiene** — close #1387 (DIM9-DEDUP-01, resolved by `b99ae91e`).
4. **Doc sync (batchable)** — fix `shader-pipeline.md` (7 attachments, RGBA16F HDR,
   336-B GpuCamera) and `memory-budget.md` (336-B camera UBO, 80%/smallest-heap
   warn threshold); fix `material.rs` "50→75 fields" comments and `volumetrics.rs`
   froxel-size comment. These are the recurring TD-class doc-rot.
5. **Hardening (optional)** — `STATIC_BLAS_FLAGS` pin test (AS-D1-01); 7-attachment
   count const + test (REN-D11-03); 64-bit caustic sum (REN-D14-NEW-01).

## Needs-RenderDoc

- **REN-D2-NEW-01 worst-case path** — whether a non-RT device fails pipeline
  creation gracefully vs executes an ungated ray query against an absent binding is
  driver-dependent and `cargo test`-invisible. Confirm on a non-RT device or via a
  capture before assuming the graceful degradation.
- **No speculative barrier edits proposed** anywhere in this sweep. Dim 4/5/11/12/14/15
  barrier observations (sync1 `ACCELERATION_STRUCTURE_READ_KHR` vs sync2
  `BUILD_INPUT_READ_ONLY_KHR`, #1436; G-buffer/compute transition masks) are
  left as observations only per standing guidance.

## Dimensions with zero findings above LOW

Dim 1 (1 LOW), 4 (clean), 6 (clean), 7 (clean, dedup #1499), 8 (clean), 9 (1 LOW
hygiene), 10 (clean), 12 (clean), 13 (TAA — prior report, 2 LOW), 17 (clean),
18 (clean), 20 (1 LOW = #1505), 21 (clean). The single LOW under Dim 20 is
**Existing: #1505** (gpu_timers.rs doc says `cmd_reset_query_pool`; actual path is
host-side `reset_query_pool`).

## Already-filed, NOT re-reported (dedup)

- **#1499 (REN2-14)** — `MaterialTable::intern` doc still says "4096 cap" + pre-split file refs (actual `MAX_MATERIALS = 16384`).
- **#1505 (REN2-20)** — gpu_timers.rs `cmd_reset_query_pool` doc vs host-side reset (counted once, under Dim 20).
- **#1427 (EGUI-03)** — EguiPass `pending_free` flush (counted once, under Dim 5).
- **#1384 (IOR-04)** — three coincident `128u` values across three separate u32 bitfield namespaces (not a real collision).
- **#1501 (REN2-16)** — DBG_* bit doc-count drift (code/pins correct).
- **#1330 (NIF-2026-05-29-02)** — BSShaderNoLightingProperty over-read (NIF-parser domain, out of renderer scope).
