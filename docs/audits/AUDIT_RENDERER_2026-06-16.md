# Renderer Audit — 2026-06-16

**Scope**: Full `/audit-renderer` pass, all 21 dimensions, `--depth deep`.
**Baseline**: HEAD `fa569908`. Prior comprehensive renderer audit
`AUDIT_RENDERER_2026-06-14.md` (largely clean; its sole HIGH — the water-path
RT gate — was subsequently fixed in `d886559c`/`1ddb7b12`, #1561). Dedup against
29 open GitHub issues + `docs/audits/`.
**Trigger context**: New work landed since the 06-14 sweep — `218b425b` (remove
ReSTIR reservoir G-buffer attachment) and `1ddeae28` (soft-particle depth fade
for effect-shader FX). Both were given focused attention; all dimensions were
still re-derived against current code, and an attempt was made to disprove each
finding before inclusion.
**Method**: CRITICAL-tier dimensions (1/2/3) run by a renderer-specialist Task
agent; the orchestrator independently verified the highest-risk recent change
(the soft-particle precision path) and the broken layout-pin tests against
source, and spot-checked the remaining dimensions (4–21) against the strong
prior-audit baseline.

---

## Executive Summary

| Severity | Count | IDs |
|----------|-------|-----|
| CRITICAL | 0 | — |
| HIGH     | 1 | REN-D10-01 |
| MEDIUM   | 1 | REN-D3-01 |
| LOW      | 2 | REN-D3-02, REN-D3-03 |
| **Total**| **4** | |

The RT core (BLAS/TLAS geometry + device addresses, `instance_custom_index` ↔
SSBO contract, GPU-struct byte layouts, ray-query plumbing, sync/barrier chains,
NIFAL material boundary, denoiser, skinning, the camera-relative precision
*cascade*) remains in a **mature, well-pinned state**. All four HIGH findings
from the 2026-06-11 audit are confirmed fixed at HEAD: **REN2-01** (skinned
raster rebase by `−render_origin`, `triangle.vert` lines 167-170), **REN2-02**
(skinned TLAS double-transform — `tlas_instance_transform` now returns IDENTITY
for skinned BLAS), and the caustic re-projection sites. The reservoir removal
(`218b425b`) is **clean in the live render path**: opaque/blend/UI pipeline
blend arrays, the render pass, the G-buffer, and the fragment-shader outputs are
all consistently **6 color attachments + depth**.

The single **HIGH** is in the brand-new **soft-particle depth-fade** code
(`1ddeae28`, shipped today): `triangle.frag` reconstructs the occluder/fragment
world positions from the **render-origin-relative** `invViewProj` but measures
the along-ray gap against the **absolute** `cameraPos.xyz`, mixing the two
precision conventions the rest of the renderer keeps strictly separate. The
feature works in interiors (where `render_origin ≈ 0`) but the gap collapses in
large-coordinate worldspaces — exactly the FO4 exterior FX volumes the feature
was built to feather.

The **MEDIUM** is a real **coverage regression** from the same reservoir-removal
refactor: two GPU-struct drift-guard tests still `include_str!("triangle.frag")`
and assert it declares `struct GpuInstance` / `GpuMaterial`, but those structs
were moved into `include/bindings.glsl`. `cargo test -p byroredux-renderer` is
**RED** (2 failures); the GLSL-side lockstep guards no longer fire. The two LOW
findings are stale "7 color attachments" comments + authoritative-doc divergence
left behind by the reservoir removal.

---

## RT Pipeline Assessment

**Acceleration structures (Dim 1)** — clean. BLAS geometry is
`R32G32B32_SFLOAT` at stride `sizeof(Vertex)`, `UINT32` index, `OPAQUE`;
`STATIC_BLAS_FLAGS`/`SKINNED_BLAS_FLAGS`/`UPDATABLE_AS_FLAGS` single-sourced in
`acceleration/constants.rs` and matched at refit (`validate_refit_flags` +
`validate_refit_counts`, VUID-03667/03708). `instance_custom_index` ==
compacted SSBO index via the shared `build_instance_map`, 24-bit ceiling
`debug_assert` at the truncation site, `MAX_INSTANCES = 0x40000` with a
`const_assert < 1<<24`. Column-major→3×4 conversion unit-pinned;
`TRIANGLE_FACING_CULL_DISABLE` gated on `two_sided`; empty TLAS valid from frame
0; `SHADER_DEVICE_ADDRESS` on all AS/scratch/instance buffers; scratch alignment
enforced (VUID-03715); LRU/shrink (#1226/#1227/#1228) wired. **REN2-02 fixed**.

**Ray queries / SSBO plumbing (Dim 2)** — clean. All hit lookups use
`rayQueryGetIntersectionInstanceCustomIndexEXT` → `instances[idx]` →
`materials[inst.materialId]` (never `gl_InstanceID`). Shadow rays biased + tMin
0.05 + `TerminateOnFirstHit`; reflection `reflect()` sign correct, metalness/
roughness-gated, miss→sky/cell-ambient; GI cosine hemisphere over the true
hit-triangle normal (#1626/#1628), interior miss→cell-ambient not sky-blue, IGN
seeded by `cameraPos.w`. Glass: per-material IOR (#1248), Frisvad basis (#820,
no normal-incidence NaN), `GLASS_RAY_BUDGET` atomic gate (#1438), window-portal
demote (#789), `DBG_VIZ_GLASS_PASSTHRU` wired. RT gating `sceneFlags.x > 0.5`
guards every ray site; `water.frag` ray sites early-out under the same gate
(#1561).

**Denoiser & skinning (Dims 8/9)** — `VERTEX_STRIDE_FLOATS = 25` is defined in
`shader_constants_data.rs` and imported via
`use crate::shader_constants::VERTEX_STRIDE_FLOATS` (not hardcoded), pinned vs
`size_of::<Vertex>()`. SVGF/composite reassembly order, caustic-into-direct
(#1575 u32-wrap guard present), Disney `MAT_FLAG_PBR_BSDF` single-gate
(`lighting.glsl`), `VOLUMETRIC_OUTPUT_CONSUMED` dispatch-skip gate, and the
`host_query_reset` gate (#1636) all spot-verified clean.

---

## GPU-Struct & Memory Assessment

GPU-struct **bytes** are intact and correct: `GpuInstance` 112 B, `GpuMaterial`
300 B (all-scalar f32/u32, no `vec3`), `GpuCamera` 336 B — the Rust-side
size/offset pins all PASS (329/331 tests green). `GpuInstance` is mirrored at 5
sites (`include/bindings.glsl` + `triangle.vert`/`ui.vert`/`water.vert`/
`caustic_splat.comp`); `triangle.frag` now `#include`s `bindings.glsl` rather
than carrying its own copy. Flag constants remain single-sourced
(`shader_constants_data.rs` → `include/shader_constants.glsl`), including the
#1500 `NORMAL_ALPHA_SPEC_BIT` migration. The **GLSL-side** drift guards,
however, are broken — see REN-D3-01.

The `render_finished` semaphore is correctly per-swapchain-image (indexed by
`image_index`, draw.rs ~L3456-3468, regression guard `548c1b69`). Memory
lifecycle, teardown ordering, and the reservoir-removal cleanup were not
re-derived exhaustively this pass (no delta beyond the attachment removal, which
is byte-consistent across pipeline/render-pass/gbuffer); they stand on the
06-11/06-14 clean baselines.

---

## Findings

### REN-D10-01: Soft-particle depth fade mixes relative/absolute precision conventions
- **Severity**: HIGH
- **Dimension**: Camera-Relative Precision
- **Location**: `crates/renderer/shaders/triangle.frag` (the `MAT_FLAG_EFFECT_SOFT` soft-particle depth-fade block, ~the `gap = length(sceneWorld - cameraPos.xyz) - length(fragSceneWorld - cameraPos.xyz)` site)
- **Status**: NEW (introduced by `1ddeae28`, this date)
- **Description**: The new soft-particle depth fade reconstructs both the
  occluder (`sceneWorld`) and the fragment (`fragSceneWorld`) by transforming
  NDC through `invViewProj`. That matrix is uploaded **render-origin-relative**
  (`GpuCamera.inv_view_proj`, built from the relative `view_proj`; confirmed by
  the doc on `GpuCamera::render_origin` and the comment at
  `context/draw.rs` ~L759: *"Uploaded `view_proj` / `inv_view_proj` are
  relative"*). So `sceneWorld`/`fragSceneWorld` land in render-origin-relative
  space. The gap is then measured against `cameraPos.xyz`, which is the
  **absolute** camera position (`effective_cam_pos`, returned unmodified by
  `dof_effective_view_proj`; `cameraPos.xyz` is the absolute world position used
  for RT ray origins everywhere else). The two conventions are mixed — the
  camera vantage point is offset from the two relative scene points by the full
  `render_origin`.
- **Evidence**: The block's own comment claims it *"mirrors `ssao.comp::worldFromDepth`
  (same `invViewProj`, same `uv*2-1` NDC)"*. But `ssao.comp` is deliberately fed
  a **relative** camera position so its `length(worldPos - cameraPos)` stays
  origin-invariant: `context/draw.rs` computes
  `ssao_cam_rel = [camera_pos[i] - render_origin.<xyz>]` and passes that to
  `ssao.dispatch(...)`, with the comment *"feed the camera in the same relative
  space."* The soft-fade path does not do this — it reuses the shared
  `CameraUBO.cameraPos` (absolute). The mirror is incomplete: same matrix, wrong
  camera space.
- **Impact**: `gap` is a *difference* of two camera distances, both computed
  from a camera point displaced by `render_origin`. In interiors
  (`render_origin ≈ 0`) the error is negligible and the feather is correct. In
  exterior worldspaces, `render_origin` is large (e.g. MarkarthWorld X ≈
  −176 000; FO4 Commonwealth exterior cells), so both `length(... - cameraPos)`
  terms are dominated by `|render_origin|` and their difference degenerates
  toward zero or noise — the soft fade either never feathers or fully dissolves
  the FX. This breaks the exact use case the feature was built for (the FO4
  HalluciGen / exterior mist volumes cited in the commit message) at any
  non-trivial render origin, while passing unnoticed in the interior test
  scenes. Not a GPU crash; bounded to effect-shader soft particles. Per the
  Dim-10 floor ("a path mixing the two conventions = HIGH"), HIGH.
- **Related**: REN2-01/REN2-03 (the 06-11 cascade fixes this commit failed to
  follow); `ssao_cam_rel` is the established correct pattern.
- **Suggested Fix**: Reconstruct in relative space against a relative camera —
  i.e. compute the gap using `cameraPos.xyz - renderOrigin.xyz` (both points and
  camera then in render-origin-relative space, differences origin-invariant),
  mirroring the `ssao_cam_rel` convention the comment already references.
  Alternatively reconstruct both scene points to absolute by adding
  `renderOrigin.xyz` before differencing against the absolute `cameraPos`.
  Either keeps the two endpoints and the camera in one consistent space.

---

### REN-D3-01: Reservoir removal broke two GPU-struct layout-pin guards (suite RED)
- **Severity**: MEDIUM
- **Dimension**: GPU-Struct Layout
- **Location**: `crates/renderer/src/vulkan/material.rs::tests::gpu_material_glsl_field_names_pinned` + `crates/renderer/src/vulkan/scene_buffer/gpu_instance_layout_tests.rs::every_shader_struct_gpu_instance_names_material_kind_slot`
- **Status**: NEW (introduced by `218b425b`)
- **Description**: Both shader-struct drift guards `include_str!` `triangle.frag`
  and assert it contains `struct GpuInstance` / the `GpuMaterial` GLSL field
  needles (e.g. `materialFlags;`). The reservoir-removal refactor extracted those
  struct declarations from `triangle.frag` into `include/bindings.glsl`
  (`triangle.frag` now `#include`s it). The tests now **panic**:
  `gpu_material_glsl_field_names_pinned` — *"expected GpuMaterial GLSL field
  needle `materialFlags;` not found"*; `every_shader_struct_gpu_instance_names_material_kind_slot`
  — *"triangle.frag no longer declares `struct GpuInstance`"*.
- **Evidence**: `cargo test -p byroredux-renderer --lib` → `329 passed; 2
  failed`. `grep -rl "struct GpuInstance" crates/renderer/shaders/` resolves to
  `bindings.glsl` + `triangle.vert`/`ui.vert`/`water.vert`/`caustic_splat.comp`
  — not `triangle.frag` (0 matches). The struct **bytes** are verified intact
  and correct in `bindings.glsl`; the Rust-side offset/size pins
  (`gpu_material_field_offsets_match_shader_contract`, `gpu_material_size_*`)
  still PASS. This is a test-fixture path bug, not real layout drift.
- **Impact**: The GLSL-side lockstep guards — the ones that catch a *shader-side*
  field rename/reorder the Rust pins cannot see — are now dead-red. A future
  shader-side `GpuInstance`/`GpuMaterial` field change in `bindings.glsl` would
  go uncaught (and a contributor may "fix" the red by deleting the needles). The
  whole `byroredux-renderer` suite being RED also masks future regressions in it.
- **Related**: `feedback_shader_struct_sync.md` (the lockstep invariant these
  guards enforce); REN-D3-02/03 (same refactor's doc lag).
- **Suggested Fix**: Repoint both tests' `include_str!` from `triangle.frag` to
  `include/bindings.glsl` (mind the relative-path depth difference between the
  two files), and update the doc-comments that cite `triangle.frag:110-184`. For
  the GpuInstance test, keep asserting the 4 `.vert`/`.comp` mirrors that still
  embed the struct and swap the `triangle.frag` entry for `bindings.glsl`.

---

### REN-D3-02: Stale "7 color attachments" comments in pipeline.rs
- **Severity**: LOW
- **Dimension**: Pipeline/RenderPass
- **Location**: `crates/renderer/src/vulkan/pipeline.rs` (comment above the opaque `color_blend_attachment` array, ~L275-277; same wording in the UI-pipeline comment ~L709)
- **Status**: NEW (`218b425b` lag)
- **Description**: Comments read *"main render pass has 7 color attachments
  (... + reservoir). Each needs a blend state entry."* The arrays directly below
  correctly have **6** entries (`0 HDR`…`5 albedo`).
- **Evidence**: opaque array = 6, blend-pipeline array = 6, UI array = 6;
  `create_render_pass` (`context/helpers.rs`) builds 6 color + depth; `gbuffer.rs`
  has 5 aux attachments (+ HDR = 6); `triangle.frag` declares 6 fragment outputs
  (locations 0-5); `reflect.rs::triangle_frag_declares_six_color_outputs` PASSES.
  Only the comments lag.
- **Impact**: None functional. Misleads a reader into expecting a 7th attachment.
- **Suggested Fix**: Update both comments to *"6 color attachments (HDR + normal
  + motion + mesh_id + raw_indirect + albedo)"*.

---

### REN-D3-03: Authoritative docs still document the removed 7th (reservoir) G-buffer attachment
- **Severity**: LOW
- **Dimension**: Pipeline/RenderPass (doc divergence)
- **Location**: `docs/engine/shader-pipeline.md` (G-Buffer Layout table + "Seven colour attachments" prose); `docs/engine/memory-budget.md` ("7 attachments × 2 FIF" VRAM line); `crates/renderer/src/vulkan/context/helpers.rs` (attachment-formats doc, "eight…the seven" wording)
- **Status**: NEW (`218b425b` lag)
- **Description**: The authoritative `shader-pipeline.md` still lists a
  `Reservoir | R32G32B32A32_UINT | ReSTIR-DI reservoir (outReservoir, location
  6)` G-buffer row and calls out "Seven colour attachments + depth";
  `memory-budget.md` VRAM table still says "G-buffer (7 attachments × 2 FIF)".
  The reservoir attachment was removed under `218b425b`/#1583; code is now 6
  attachments. The audit instruction set treats divergence from these
  authoritative docs as a finding.
- **Evidence**: `218b425b` removed the reservoir from `gbuffer.rs`/render
  pass/pipeline/`reflect.rs`/shaders but did not touch the two reference docs.
- **Impact**: None functional. A reader sizing VRAM or wiring a new pass off
  these docs would over-count one `R32G32B32A32_UINT` attachment (16 B/px × 2
  FIF) and hunt for a non-existent location-6 output.
- **Suggested Fix**: Delete the Reservoir row from the shader-pipeline.md
  G-Buffer table and change "Seven" → "Six"; update memory-budget.md "7
  attachments" → "6" and the dependent MB figures; fix the helpers.rs
  "eight…seven" doc wording.

---

## Prioritized Fix Order

1. **REN-D10-01 (HIGH, correctness)** — fix the soft-particle precision-space
   mix before it ships into exterior-FX cells; one-line space alignment using
   the existing `ssao_cam_rel` pattern.
2. **REN-D3-01 (MEDIUM, restores RED suite + drift coverage)** — repoint the two
   `include_str!`s to `bindings.glsl`; restores `cargo test -p byroredux-renderer`
   to green and re-arms the GLSL lockstep guard.
3. **REN-D3-02 / REN-D3-03 (LOW, doc hygiene)** — clean up the stale
   "7 attachments" comments and the two authoritative-doc rows in one pass.

## Needs-RenderDoc

None this pass. The soft-particle depth-fade path (REN-D10-01) is a *shader
arithmetic* bug observable by reasoning about the upload conventions, not a
barrier/layout issue — no capture required. The new D32 depth-history image's
copy barriers and layout transitions were reported clean under validation
layers by the authoring commit (`1ddeae28` message: "GPU barriers / layout
transitions verified with Vulkan validation layers on (zero errors)"); they
were not independently re-captured here and are out of scope for a static pass.

---

*Generated by `/audit-renderer` (deep). Suggested next step:*
`/audit-publish docs/audits/AUDIT_RENDERER_2026-06-16.md`
