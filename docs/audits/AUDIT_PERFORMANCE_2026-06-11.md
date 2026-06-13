# Performance Audit — ByroRedux — 2026-06-11

**Command**: `/audit-performance --focus 1,2,3,7,8` (depth deep) — part of `/audit-suite --preset renderer-deep`
**Trigger**: camera-relative rendering precision work (`36f66493`, `bccf06f0`)
**Method**: 5 dimension agents (renderer specialists) + orchestrator verification, read-only, exact line citations. Every finding's premise verified against current code before inclusion.
**Dedup baseline**: 33 open GitHub issues (`/tmp` snapshot 2026-06-11); prior `AUDIT_PERFORMANCE_*` reports through 2026-06-04.
**Dimensions audited**: 1 (GPU Pipeline), 2 (GPU Memory), 3 (Draw Call Overhead), 7 (TAA & GPU Skinning), 8 (Material Table & SSBO Upload). Dimensions 4–6, 9, 10 out of scope for this run.

---

## Executive Summary

| Severity | Count (deduplicated) |
|---|---|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 2 |
| LOW | 5 |

**Headline: the camera-relative rendering cascade is verified clean end-to-end.** This audit was triggered by the large-world precision work, and every interaction point checked out:

- **CPU side** — per-instance rebase is O(instances) trivial subtraction; CPU cull/sort correctly keeps the absolute `vp_mat` + frustum; batch merge keys are unaffected by the rebase (M31 batching intact).
- **Upload side** — `GpuCamera` grew to 336 B with the `gpu_camera_is_336_bytes` pin; `renderOrigin` is present in all 7 GpuCamera shader mirrors; `ssao.comp`/`composite.frag` correctly use their own relative `camera_pos` (origin-invariant differences). The instance-SSBO dirty gate survives: within a 4096-unit grid cell the snapped origin is constant, so static scenes still hash-match every frame.
- **RT/AS side** — the rebase touches ONLY the raster GpuInstance translation and camera UBO. TLAS uses absolute `draw_cmd.model_matrix` (tlas.rs:180), skinned BLAS uses absolute `bone_world` (skinned.rs:162), and `triangle.vert:190` reconstructs absolute `fragWorldPos` for the ray query against the absolute TLAS. No double-rebase, no relative/absolute mismatch.

**All must-not-regress baselines verified intact** across dims 7 and 8: M29.5 palette compute pass, M29.6 persistent bind-inverses + first-sight upload, #1195 dispatch-dirty gate (first-sight invariant honored), #1196 three-conjunct BLAS-refit gate, #1197 descriptor-rewrite skip, #1194 instrumentation, TAA O(pixels), R1 NIFAL pin (no per-draw `classify_pbr_keyword`), GpuInstance 112 B (render_origin landed on GpuCamera, not GpuInstance), FxHash intern (#1368), dirty-gated O(unique) material upload (#878).

The two MEDIUM findings are GPU-efficiency items: a hoistable 5×5 Gaussian weight recomputation inside the caustic per-light loop, and the (carried-over, now fully quantified) write-only ReSTIR reservoir G-buffer cost at higher resolutions.

---

## Hot-Path Analysis (per-frame)

| Per-frame operation | Cost / scaling | Status |
|---|---|---|
| Per-instance camera-relative rebase | O(instances), 3 subs each | clean (new, verified) |
| Instance SSBO upload after origin snap | 1-frame full re-upload per 4096 u of travel (~530 KB worst case) | F3 (LOW, by design) |
| Caustic splat 5×5 weight normalization | up to N_LIGHTS × 50 `exp()` per caustic pixel, all loop-invariant | **F1 (MEDIUM)** |
| G-buffer write bandwidth | 36 B/px across 7 attachments (16 B/px is the write-only reservoir) | **F2 (MEDIUM, carry-over N1)** |
| Material dirty-gate hash | O(unique materials) FxHash even on skip (~40 KB @200 mats, sub-µs) | F7 (LOW, deliberate) |
| Skin dispatch / BLAS refit | gated by `pose_dirty` + first-sight; skipped when pose unchanged | clean (#1195/#1196 intact) |
| TAA / SVGF / bloom / volumetrics | O(pixels) / O(froxels), single dispatches | clean |
| Draw recording loop | binds only on state change; zero per-draw allocations | clean |
| Draw list sort | `par_sort_unstable_by_key` ≥2 K draws, mesh-grouping key | clean |

---

## Findings

### MEDIUM

#### F1 — Caustic 5×5 Gaussian splat weights recomputed per-light (PERF1-01)
- **Severity**: MEDIUM
- **Dimension**: GPU Pipeline
- **Location**: `crates/renderer/shaders/caustic_splat.comp:404-414`
- **Status**: NEW
- **Description**: The 5×5 Gaussian splat weight normalization (`wsum` loop, 25 `exp()` calls at lines 404-407) and the per-tap weight `exp()` (line 412) live INSIDE the per-light loop `for (uint li...)` opened at line 258. σ (=1) and the ±2 support are compile-time fixed, so `wsum` is fully loop-invariant and all 25 tap weights are constants. Introduced with the 5×5 footprint in `afaa2fe4`/`73eb7663`.
- **Evidence**: None of the weight inputs (`kx`, `ky`) depend on `li`; with N_LIGHTS up to maxLights, the 25-tap normalization sweep plus up to 25 per-tap `exp()` are redone every light iteration for every caustic-source pixel.
- **Impact**: Up to N_LIGHTS × 50 transcendentals per caustic pixel where 0 are needed at runtime. `exp()` is a multi-cycle SFU op; on a dense water scene with multiple contributing lights this is a measurable compute-pass cost with zero correctness benefit.
- **Suggested Fix**: Replace with a `const float kGauss5[25]` of pre-normalized weights (sum 1), indexed `(ky+2)*5+(kx+2)` — removes the `wsum` loop and all runtime `exp()`. Minimum: hoist the `wsum` computation above the light loop.

#### F2 — Write-only ReSTIR reservoir attachment: full G-buffer + SVGF VRAM budget quantified (PERF2-03)
- **Severity**: MEDIUM
- **Dimension**: GPU Memory
- **Status**: Carry-over of prior-audit N1 (2026-06-04, unpublished — no GH issue yet) with new quantification
- **Location**: `crates/renderer/src/vulkan/gbuffer.rs:63,283-287`
- **Description**: The 7th G-buffer attachment (`R32G32B32A32_UINT` ReSTIR reservoir, 16 B/px) remains write-only — no resample pass reads it. New angle: full G-buffer + SVGF history budget computed for the dev card's likely targets (FIF=2, 36 B/px G-buffer total):
  - 1080p: G-buffer 149 MB (reservoir 66 MB) + SVGF 50 MB ≈ **199 MB**
  - 1440p: G-buffer 265 MB (reservoir 118 MB) + SVGF 89 MB ≈ **354 MB**
  - 4K: G-buffer 597 MB (reservoir 265 MB) + SVGF 199 MB ≈ **796 MB**
  The write-only reservoir alone is a third of the G-buffer.
- **Impact**: Works against the < 4 GB VRAM target, especially at 1440p+; pure dead weight until the Phase 2 resample pass lands.
- **Suggested Fix**: (a) Gate the reservoir attachment behind a feature flag until the reader pass exists. (b) When wired, consider single-buffering it if it stays transient within a frame (halves the cost). (c) Optionally `mesh_id` R32_UINT → R16_UINT if mesh count < 65 K.

### LOW

#### F3 — Instance SSBO re-upload burst at render-origin snap boundaries (PERF2-01)
- **Severity**: LOW
- **Dimension**: GPU Memory
- **Location**: `crates/renderer/src/vulkan/context/draw.rs:581-585,1751-1769` + `crates/renderer/src/vulkan/scene_buffer/upload.rs:489-516`
- **Status**: NEW (informational — design is correct)
- **Description**: The camera-relative rebase subtracts `render_origin` from every instance's model translation. The instance-SSBO dirty gate (#1134) is a content hash, so the frame the camera crosses a 4096-unit snap line, every instance's bytes change and the full slice re-uploads (~530 KB at MedTek scale: 7359 × 72 B). Within a grid cell the origin is constant and the gate works as before.
- **Impact**: One-frame burst per 4096 units of camera travel — ~530 KB/s during fast exterior flight, well under the ~32 MB/s the gate was built to prevent. No sustained regression.
- **Suggested Fix**: None required. If exterior-flight churn ever matters, keeping instance models cell-local with the origin applied purely in the camera UBO would eliminate snap re-uploads (larger redesign; not warranted now).

#### F4 — BLAS build-scratch only shrinks on window resize, not on cell unload (PERF2-02)
- **Severity**: LOW
- **Dimension**: GPU Memory
- **Location**: `crates/renderer/src/vulkan/context/resize.rs:38-44` (sole call site of `shrink_blas_scratch_to_fit`)
- **Status**: NEW
- **Description**: The per-frame path shrinks TLAS, TLAS-scratch, and CPU scratches (`draw.rs:3385-3431`) but the BLAS build-scratch shrink (`acceleration/memory.rs:42`) is invoked only from `recreate_swapchain`. A one-off large BLAS build (80-200 MB, the #495 failure mode) stays resident until the next resize. The 2x + 16 MB hysteresis (`predicates.rs:262`) prevents churn but anchors to the post-eviction peak.
- **Impact**: Up to ~200 MB of idle scratch across a session on a no-resize run; fine on 12 GB, works against the < 4 GB target.
- **Suggested Fix**: Also call `shrink_blas_scratch_to_fit` after cell-unload / large-eviction events, where a sync point is already paid.

#### F5 — SVGF moments history wastes its 4th RGBA16F channel (PERF2-04)
- **Severity**: LOW
- **Dimension**: GPU Memory
- **Location**: `crates/renderer/src/vulkan/svgf.rs:14-15,89`
- **Status**: NEW (informational)
- **Description**: `MOMENTS_HIST_FORMAT = R16G16B16A16_SFLOAT` stores (μ₁, μ₂, history_length, *unused*). ~5.5 MB @1080p / ~22 MB @4K (×2 FIF) of dead channel.
- **Suggested Fix**: Effectively unavoidable (no universal 3-channel 16F format); document as known. Not worth a separate R16 image.

#### F6 — Render-origin snap formula duplicated across a crate boundary (PERF3-01)
- **Severity**: LOW
- **Dimension**: Draw Call Overhead
- **Location**: `byroredux/src/render/camera.rs:97,156` + `crates/renderer/src/vulkan/context/draw.rs:581-585`
- **Status**: NEW
- **Description**: Both sites define their own `const RENDER_ORIGIN_SNAP: f32 = 4096.0` and independently compute `(pos / SNAP).floor() * SNAP`. The camera-relative cascade *requires* bit-identical origins (relative `view_proj` in camera.rs vs rebased instances in draw.rs) — the only guard is a `// MUST match` comment. Identical today; a future edit to one site silently desyncs GPU origin from CPU-rebased instances, with geometry shifted by up to 4096 units, invisible to `cargo test` (needs Vulkan + large-world content).
- **Suggested Fix**: Hoist to a single shared `pub const RENDER_ORIGIN_SNAP` + `fn snap_render_origin(pos: Vec3) -> Vec3` (e.g., on `GpuCamera` or in core math), called from both sites.

#### F7 — Material dirty-gate FxHashes the full table every frame even when skipping (PERF8-05)
- **Severity**: LOW
- **Dimension**: Material Table & SSBO Upload
- **Location**: `crates/renderer/src/vulkan/scene_buffer/upload.rs:554` + `descriptors.rs:220-234` (`hash_material_slice`)
- **Status**: NEW (informational — deliberate, correct tradeoff)
- **Description**: The upload gate hashes `materials[..count]` every frame (O(count × sizeof GpuMaterial)); at ~200 materials this is ~40 KB of FxHash — sub-microsecond and far cheaper than the avoided PCIe copy. Linear-scaling note only: near MAX_MATERIALS on very large worldspaces, a generation counter on `MaterialTable` (bump on intern miss) would replace the byte hash with an O(1) compare.
- **Impact**: Negligible today.

---

## Baselines Verified (all dimensions, must-not-regress)

### Dimension 1 — GPU Pipeline
- Camera-relative cascade clean GPU-side: origin re-added exactly once per consumer stage (`triangle.vert`, `cluster_cull.comp`, `volumetrics_inject.comp`, `caustic_splat.comp`); no back-and-forth conversions.
- TLAS build→ray-query barriers correct; refit-vs-rebuild policy intact.
- Volumetrics pure O(froxels) (160×90×128); bloom pure O(pixels) (5 down + 4 up).
- SVGF single temporal dispatch; TAA single dispatch, per-pixel motion-gated luma-clamp skip (`2f7bcf78`), no per-mesh scaling.
- Progressive parked-camera accumulation (`a7f7f0f4`/`c7ca4864`) = seed change only; glass/IOR two-surface path gated off opaque fragments.
- Mesh-index validation (`01251733`) is load-time (`MeshRegistry::register`), not per-draw.
- Draw-loop state caching: pipeline / depth-bias / depth test/write/compare emitted only on change (`draw.rs:2446-2516`).
- Not re-reported (existing): #1369 (WRS reservoir loop, partially addressed), #1438 (ray-budget atomicAdd).

### Dimension 2 — GPU Memory
- CameraUBO 336 B lockstep: `gpu_camera_is_336_bytes` pin (`gpu_instance_layout_tests.rs:50-54`); `renderOrigin` in all 7 GpuCamera mirrors (triangle.vert/.frag, water.vert/.frag, cluster_cull, caustic_splat, volumetrics_inject); ssao/composite use separate param structs with relative `camera_pos` — no drift.
- Mapped writes: `CpuToGpu` with cached HOST_COHERENT flag + explicit `flush_mapped_memory_ranges` on non-coherent paths (`buffer.rs:405,627-748`).
- Texture staging: `StagingPool` reuse via `StagingGuard::release_to` (`texture.rs:47-151`).
- Teardown (`04acaa2b`): happy path drops Allocator before device destroy; `try_unwrap`-fails path is the documented leak-not-UAF mitigation (#1406/#665). #1426 remains the known open issue.
- Water blend pipeline (`40f90efc`): 7 blend entries match 7 color attachments; integer reservoir blend disabled (VUID-04727 documented, `water.rs:599-612`).
- TLAS instance buffer 2x padding bounded (per-frame shrink + 8192 floor).

### Dimension 3 — Draw Call & Batching
- Sort key (`draw_sort_key`, `render/mod.rs:187`): RT-only last (overflow-drop order preserved), opaque mesh-grouped front-to-back, blended back-to-front, deterministic tiebreak; `par_sort_unstable_by_key` ≥2 K draws.
- M31 instanced merge intact (`draw.rs:1844-1858`) with SSBO-gap guard (`:1789`) — no index-mismatch risk; post-merge batch telemetry (#1258).
- Zero per-draw allocations (String/Vec/clone/log) in `static_meshes.rs` and the recording loop; descriptor sets bound once per pass (`draw.rs:2283-2294`).
- R1/`44171cd5` pin: no per-draw classification; `normal_alpha_spec_applies` is 5 scalar compares at spawn.
- CPU cull/sort uses absolute `vp_mat` + frustum; rebase happens after batch-key formation — batching unaffected.

### Dimension 7 — TAA & GPU Skinning (all 8 baselines OK)
- M29.5 dedicated `SkinPaletteComputePipeline` (skin_compute.rs:689); no inline bind-inverse multiply in `skin_vertices.comp`.
- M29.6 persistent bind-inverses SSBO, O(first-sight) upload (`upload.rs:267/313`, drained `draw.rs:848-868`).
- #1195 dispatch-dirty gate live (`draw.rs:1237-1241`), first-sight invariant honored (`has_populated_output` flip at `draw.rs:1258`); `clear_pose_dirty` before loop (skinned.rs:152), `try_mark_pose_dirty` inside (skinned.rs:180).
- #1196 three-conjunct refit gate live (`draw.rs:1382-1392`).
- #1197 descriptor-rewrite skip + `descriptor_writes_this_frame` counter (skin_compute.rs:540,564,605).
- #1194 GPU timers + `dispatches_skipped` wired (gpu_timers.rs; draw.rs:1216/1239/1264).
- TAA dispatch pure pixel grid (`taa.rs:771-773`); history 2× RGBA16F (≈64 MB @4K, noted).
- Refit dominates; rebuild only on bone-count change (blas_skinned.rs:477); statics never dispatch.
- **Camera-relative × skinning/RT: CLEAN** — TLAS/skinned-BLAS/ray origins all consistently absolute (see Executive Summary). #1387 remains the only known open skinning issue (not re-reported).

### Dimension 8 — Material Table & SSBO Upload (all 5 baselines PASS)
- NIFAL pin: all `classify_pbr_keyword` call sites import/spawn/test; draw loop reads plain `f32` metalness/roughness (material.rs:217,223); `resolve_pbr` idempotent; recent commits `f211bf1f`/`83d6a155`/`44171cd5` touch classifier definitions only.
- GpuInstance still 112 B — all three pin tests assert; `render_origin` landed on GpuCamera (`gpu_types.rs:273`), GpuInstance unchanged.
- `MaterialTable::intern` FxHash O(1) amortized (#1368 intact); N placements → 1 GpuMaterial.
- Per-frame upload O(unique), content-hash dirty-gated per FIF, hash stamped post-flush; buffers allocated once for MAX_MATERIALS and reused (#878 gate intact).

---

## Prioritized Fix Order

1. **F1** (quick win, shader-only): const-fold the caustic 5×5 Gaussian weights — removes up to N_LIGHTS×50 SFU ops per caustic pixel. Recompile `caustic_splat.comp` with plain `glslangValidator -V`.
2. **F6** (quick win, refactor): shared `snap_render_origin` helper — removes the cross-crate "MUST match" foot-gun while the camera-relative code is fresh.
3. **F4** (small): hook `shrink_blas_scratch_to_fit` into cell-unload/eviction events.
4. **F2** (decision needed): feature-gate the reservoir attachment until the ReSTIR resample pass lands, or accept the VRAM as Phase-2 staging. Publish prior N1 as a GitHub issue so it stops being re-found.
5. **F3 / F5 / F7**: informational — document, no action warranted today.

**Note on alloc findings**: the dhat / alloc-counter infrastructure gap (#1381) persists — no CPU-allocation findings were in scope for this focused run (dims 4-6 not audited), but the gap still applies to any future alloc fixes.
