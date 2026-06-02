# Renderer Audit — 2026-06-02

**Scope:** Full 23-dimension sweep per `/audit-renderer`, depth=deep.
**Method:** 8 parallel dimension agents, each re-reading code paths and disproving
findings before reporting; deduped against 59 open issues + prior `docs/audits/`
renderer reports.

---

## Executive Summary

The renderer is **exceptionally well-hardened**. 21 of 23 dimensions are
steady-state clean — nearly every checklist item maps to a closed-issue
regression guard that remains intact and, in most cases, test-pinned or
release-effective-`assert`-pinned (not just `debug_assert`). The acceleration,
sync, memory, denoiser, TAA, material-table, NIFAL, and tangent-space subsystems
carry no actionable defects.

**One HIGH finding**, and it is the headline: the committed SPIR-V binaries for
5 shaders are **stale** relative to their GLSL after the depth-of-field commit
(`400fa68f`). Benign in today's render output (the new UBO field is trailing and
unread) but a latent landmine with **zero test/validation coverage** — the next
mid-struct `CameraUBO` edit silently corrupts every camera-UBO consumer.

### Severity rollup

| Severity | Count | Findings |
|----------|-------|----------|
| CRITICAL | 0 | — |
| HIGH     | 1 | PIPE-01 / SHDR-01 (stale committed `.spv`) |
| MEDIUM   | 0 | — |
| LOW (NEW)| 4 | SYNC-01, MEM-01, SKIN-02, WAT-01 |
| Existing (re-confirmed) | 8 | #1438, #1384, #1407, #1445, #1382, #1098(closed), #1427, #1433, #1376 |
| Clean / steady-state | 18 dims | — |

### Independent side-result

Two agents (Dim 15, Dim 23) independently verified this session's earlier
**#1410 lock-ordering fix** is correct and behavior-preserving: `weather_system`
writes `SkyParamsRes`/`CloudSimState`/`CellLightingRes` in three sequential
non-nested scopes (`weather.rs:507-580`), and `render/lights.rs:68-93` snapshots
`sun_intensity` before acquiring `CellLightingRes` — the ABBA cycle is gone with
no value change.

---

## RT Pipeline Assessment

**Clean.** BLAS/TLAS correctness is release-effective-enforced against the full
Vulkan AS VUID family (03667 BUILD/UPDATE flag match, 03708 primitiveCount match,
03715 scratch alignment, 03801 BUILD flags) at every build/refit site, with
scratch-serialize `AS_WRITE→AS_WRITE` barriers incl. the cross-submission case
validation can't catch. Ray-query shaders: V-aligned `N_bias` + tMin 0.05 (no
self-intersection), Frisvad orthonormal basis at every jitter site (#820),
`GLASS_RAY_BUDGET=8192` wired, `TerminateOnFirstHit` on all visibility rays,
exactly 2 `MAT_FLAG_PBR_BSDF` gate sites with `distributionGGXAniso` proven to
reduce exactly to `distributionGGX` at `ax==ay`. SVGF reprojection MV convention
matches `triangle.frag`, mesh-ID + normal-cone disocclusion, ACES applied last,
fog post-tonemap (never baked into denoised history). The only RT-area item is
the bounded, self-terminating ray-budget counter inflation already tracked as
**#1438**.

## Rasterization Assessment

**One real issue (PIPE-01).** Pipeline state, render pass, G-buffer, and command
recording are otherwise clean: vertex-input↔shader layout test-pinned, 6-color +
depth render pass with correct CLEAR+STORE / layouts / subpass deps, all G-buffer
formats match shader output types (RG16_SNORM octahedral normals, R16G16_SFLOAT
motion, R32_UINT mesh-id with bit-31 `ALPHA_BLEND_NO_HISTORY`), frame-sync
machinery (`render_finished` per-swapchain-image, both-slot fence wait,
all-SIGNALED resize recovery) sound, and reverse-order Drop teardown correct.

---

## Findings

### PIPE-01 / SHDR-01: Committed SPIR-V is stale vs GLSL after the DoF commit
- **Severity**: HIGH
- **Dimension**: Pipeline State / Shader Correctness
- **Location**: `crates/renderer/shaders/{triangle.vert,triangle.frag,cluster_cull.comp,water.vert,caustic_splat.comp}.spv`; root cause commit `400fa68f`; ship path `crates/renderer/src/vulkan/pipeline.rs:9-10` + `compute.rs:18` (`include_bytes!`)
- **Status**: NEW
- **Description**: Commit `400fa68f` ("stochastic depth of field", 2026-06-01)
  appended `vec4 dofParams` to the `CameraUBO` block in 6 shader sources and grew
  Rust `GpuCamera` 304→320 B (`scene_buffer/gpu_types.rs:257`), and even updated
  the size test — but committed **no recompiled `.spv`**. `crates/renderer/build.rs`
  does not compile GLSL→SPIR-V (warning-only); shaders ship as committed binaries
  via `include_bytes!`, so the **stale binary is the runtime shader**.
- **Evidence**: Recompile + `cmp` confirms 5 committed `.spv` differ from their
  current GLSL: `triangle.vert`, `triangle.frag`, `cluster_cull.comp`,
  `water.vert`, `caustic_splat.comp` are STALE; `water.frag`, `composite.*`,
  `ssao.comp`, `svgf_temporal.comp`, `taa.comp`, `skin_vertices.comp` are CURRENT.
  `git show --stat 400fa68f` shows `triangle.frag | 3 +-` (GLSL) with no `.spv`
  in the file list.
- **Impact**: **Benign today** — `dofParams` is the trailing `CameraUBO` member,
  read 0 times in every shader (DoF is applied CPU-side via view-matrix
  displacement), and an over-sized UBO bind is spec-legal (no validation error,
  no artifact). **Latent CRITICAL**: the next mid-`CameraUBO` field insertion, or
  any shader read at/after offset 304, silently corrupts every camera-UBO consumer
  (position, view/proj, jitter, sky_tint, sun_direction) with **zero** test or
  validation-layer coverage. `reflect::validate_set_layout` checks binding
  index/type only, not member bytes, so it cannot catch this.
- **Related**: this is the same class as the shader-struct-sync hazard in
  `feedback_shader_struct_sync.md` (GpuInstance lockstep across 5 shaders), but on
  the *compiled-output* axis rather than the source axis.
- **Suggested Fix**: (1) Recompile + commit the 5 stale `.spv`. (2) Add a
  `cargo test` that recompiles each `shaders/*.{vert,frag,comp}` with
  `glslangValidator` and `cmp`s against the committed `.spv` — fail on drift. This
  closes the structural gap (`build.rs` being warning-only) that let source and
  binary diverge silently.

### SYNC-01: Screenshot readback uses stale extent after a same-frame resize
- **Severity**: LOW
- **Dimension**: Vulkan Sync
- **Location**: `crates/renderer/src/vulkan/context/screenshot.rs:16-72` (readback) vs `:79-173` (record)
- **Status**: NEW
- **Description**: `screenshot_record_copy` sizes staging + copy region at
  frame-N extent and sets `screenshot_pending_readback`; `screenshot_finish_readback`
  at frame N+1 re-reads `self.swapchain_state.extent` (possibly changed by a
  `recreate_swapchain` between the two). Neither `recreate_swapchain` nor the
  resize path clears the pending flag or invalidates staging.
- **Evidence**: `screenshot.rs:26-27` re-derives width/height from the live extent;
  `:52` `write_image` uses them over old-extent staging data. Slice read is
  bounds-checked (`&slice[..size]`) — not memory-unsafe.
- **Impact**: A screenshot requested on a resize frame produces a corrupt /
  mis-dimensioned PNG. `byro-dbg` tooling only; not the render hot path.
- **Suggested Fix**: Capture `(width,height)` into the `screenshot_staging` tuple
  at record time and read those back; or clear `screenshot_pending_readback` in
  `recreate_swapchain`.

### MEM-01: `evict_unused_blas` immediate-destroy assumes no in-flight TLAS during multi-batch cell load
- **Severity**: LOW (latent; gated behind a future refactor)
- **Dimension**: GPU Memory / Acceleration Structures
- **Location**: `crates/renderer/src/vulkan/acceleration/blas_static.rs:991-1073` (`evict_unused_blas`), invoked mid-batch `:507-521`; `frame_counter` bump `:397`
- **Status**: NEW (observation; no code change recommended now)
- **Description**: `evict_unused_blas` destroys the AS immediately (no
  `pending_destroy_blas` round-trip), safe via the `MIN_IDLE_FRAMES` const-assert.
  During a multi-batch cell load, `build_blas_batched` bumps `frame_counter` per
  batch without `draw_frame`/`build_tlas` between batches, so a BLAS still
  referenced by the in-flight previous TLAS could read as `idle` and be destroyed.
  **Not reachable today** — cell loads are gated behind the load flow, not
  interleaved with live rendering.
- **Impact**: Theoretical TLAS-referenced-BLAS use-after-free **only** if a future
  streaming-during-render refactor runs `build_blas_batched` while frames are
  genuinely in flight.
- **Suggested Fix**: Add a one-line invariant note on `evict_unused_blas`; route it
  through `pending_destroy_blas` (as `drop_blas` already does) if
  streaming-during-render lands.

### SKIN-02: Stale `MAX_TOTAL_BONES` value in doc comment
- **Severity**: LOW (doc-rot)
- **Dimension**: GPU Skinning
- **Location**: `crates/renderer/src/vulkan/scene_buffer/constants.rs:69`
- **Status**: NEW
- **Description**: Doc comment cites `MAX_TOTAL_BONES (32768)` / derived ceiling 227;
  the live const is `196608` (ceiling 1365, which `skin_compute.rs:343` already
  states correctly). Cosmetic only; code reads the live symbol.
- **Suggested Fix**: Update the stale comment. Fold into the doc-rot cleanup batch.

### WAT-01: Submersion state has no hysteresis band (low-confidence)
- **Severity**: LOW
- **Dimension**: Water
- **Location**: `byroredux/src/systems/water.rs:92` (`head_submerged = depth > 0.0`)
- **Status**: NEW (design observation, not a regression)
- **Description**: The submersion flip has no hysteresis; underwater FX could strobe
  if the camera is parked exactly at the waterline — not a normal gameplay state.
- **Suggested Fix**: None pre-emptively without a repro; add a small hysteresis band
  if strobing is ever observed.

---

## Existing issues re-confirmed (deduped, not re-filed)

| ID | Dim | Note |
|----|-----|------|
| **#1438** (RT-01) | RT Ray Queries | IOR ray-budget `atomicAdd` fires unconditionally; rejected fragments never refund → counter inflates. Bounded/self-terminating. Re-confirmed `triangle.frag:2122-2125`. |
| **#1384** (PBR-01 / NORM-DBG-01) | PBR / Normals | `DBG_VIZ_GLASS_PASSTHRU` = `MAT_FLAG_MODEL_SPACE_NORMALS` = `INSTANCE_FLAG_FLAT_SHADING` = `128u`. Confirmed **namespace-isolated** (no site ANDs the wrong word) — readability hazard only, not a bug. |
| **#1407** (R1-MAT-DOC-01) | Material Table | `intern()` doc still cites the old `4096` cap / moved line ref; live cap `MAX_MATERIALS=16384`. Doc-only. |
| **#1445** (NIFAL-PART-01) | NIFAL | `planar_angle` omitted from emitter finite sweep — harmless (never copied downstream). |
| **#1382** (NIFAL-PART-02) | NIFAL | `emit_particles` divides by `lifes[i]` without in-site zero-guard; mitigated by import-side `life_span>0` reject + spawn `.max(0.05)`. |
| #1098 (CAU-01) | Caustics | Caustic reads `flags`/`avgAlbedo` from the `GpuInstance` SSBO copy, not `materials[material_id]` — documented deferred R1 state, instance copy authoritative today. (Issue closed; deferred-by-design.) |
| **#1427** (EGUI-03) | Debug Overlay | `EguiPass::destroy()` doesn't flush `pending_free` before Renderer drop. |
| **#1433** (EGUI-04) | Debug Overlay | egui RP has no outgoing EXTERNAL subpass dependency (relies on implicit external dep). |
| **#1376** (PERF) | Debug Overlay | `build_debug_ui_snapshot` clones metrics every frame even when overlay hidden. |

## Audit-brief corrections (stale skill annotations, for skill maintenance)

- **GI distance cutoff** is **4000–6000 BU** in current `triangle.frag`, not the
  "1500 units" stated in Dim 9 / Dim 15.
- **`MAT_FLAG_*` bits 5–9 ARE now in** `shader_constants_data.rs:124-133` (migrated
  post-#1285); the Dim 21 brief's "NOT in shader_constants_data.rs today" is stale.
- **Doc-rot**: `context/helpers.rs:67,72` cites `triangle.frag:980` + a
  `debug_assert!` overrun guard for mesh-id encoding; live code is at
  `triangle.frag:1355-1357` (line drift, warn+clamp not assert).
- The `GLSL-PathTracer` reference repo is **missing** from `/mnt/data/src/reference/`
  on this machine, so Dim 21 preset values were checked against in-source citations
  only (internally consistent).

---

## Prioritized Fix Order

1. **PIPE-01 / SHDR-01 (HIGH)** — recompile + commit the 5 stale `.spv`, and add a
   GLSL↔`.spv` drift `cargo test`. This is both a correctness fix (binaries now
   match source) and a structural guard against recurrence. The single most
   important item in this report.
2. **SYNC-01, SKIN-02 (LOW)** — small localized fixes (screenshot extent capture;
   doc-rot), batchable.
3. **MEM-01 (LOW/latent)** — add the invariant note now; defer the
   `pending_destroy_blas` route until streaming-during-render is on the roadmap.
4. Doc-rot batch: #1407, SKIN-02, the brief-correction notes above.
5. Existing tracked issues (#1438, #1427, #1433, #1376) — already filed; no action
   in this report.

---

*Generated by `/audit-renderer`. To file the NEW findings as issues:*
```
/audit-publish docs/audits/AUDIT_RENDERER_2026-06-02.md
```
