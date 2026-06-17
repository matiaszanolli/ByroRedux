# Performance Audit — 2026-06-16

**Command**: `/audit-performance --focus 1,2,3,5 --depth deep`
**Scope**: Dimensions 1 (CPU Per-Frame Allocations & Hot Paths), 2 (Draw-Call &
Instancing Efficiency), 3 (GPU Memory Pressure & Eviction Thrash), 5 (GPU
Pipeline & Pass Efficiency). Dimensions 4, 6, 7, 8, 9 were explicitly out of
scope for this pass.
**Method**: 4 dimension agents (renderer / general specialists) + orchestrator
merge. Every finding's premise re-verified against current `main` (HEAD
`fa569908`) before inclusion; stale-premise findings dropped.
**Hardware target**: RTX 4070 Ti (12 GB) + Ryzen 7950X (16c/32t).
**Dedup baseline**: 29 open GitHub issues (`/tmp` snapshot 2026-06-16); prior
performance reports through `AUDIT_PERFORMANCE_2026-06-14.md`.
**Bench-of-record**: ROADMAP carries the stale `R6a-*` block (flagged
non-gating, 201 commits stale). No fresh bench was run for this report; findings
are static-analysis deltas, not measured FPS regressions.

---

## Executive Summary

The four audited dimensions are in **strong shape**. Across dims 1, 2, 3, 5 this
pass found **one MEDIUM** and **two LOW** findings — all of them
enhancement/hygiene class, none correctness-threatening, none a leak or a
spec violation.

| Severity | Count |
|---|---|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 1 |
| LOW | 2 |

The dominant outcome of this pass is **confirmation that recently-filed
findings and Session-46/47 guards have all landed or held**:

- **F1 (draw-group merge, HIGH, 2026-06-14) — RESOLVED** by commit `673e02e1`
  (#1581). The indirect-merge key now includes `two_sided` + full depth state
  (`group_state` returns `(pipeline_key, render_layer, two_sided, z_test,
  z_write, z_function)`), so a leader's cull/depth can no longer bleed across a
  state boundary. Pinned by `group_state_tests`.
- **F5 (`about_to_wait` two fresh `HashSet`s/frame, MEDIUM, 2026-06-14) —
  RESOLVED** by #1584: persistent `in_use_mesh_scratch` / `in_use_tex_scratch`
  reused via `clear()`. The prior line numbers no longer match.
- **F2 / PERF1-01 (caustic per-light `exp()`, MEDIUM, re-affirmed twice) —
  RESOLVED** by commit `e8a7a386` (#1582): the 5×5 Gaussian kernel
  (`kGauss5[25]`) is now computed once before the per-light loop.
- **F4 / PERF2-02 (BLAS build-scratch shrink-on-resize-only) — RESOLVED**:
  `shrink_blas_scratch_to_fit` now also fires from cell-unload
  (`cell_loader/unload.rs:134`).
- **#1583 (write-only ReSTIR-DI reservoir attachment, MEDIUM, OPEN) — STALE,
  should be CLOSED**: the attachment was removed by commit `218b425b`. The
  G-buffer is now 6 color + depth (down from 7); no dead reservoir VRAM
  remains. See D5 conflict-resolution below.

All Session-46 CPU guards (#1371 `drain_dirty_into`, #1372 animation scratch,
#1374 billboard `last_cam` gate, #1376 debug-UI snapshot gate, #1379 bone-pool
contraction) and the #1377 GT-presence hoist verified **intact**.

> **dhat caveat**: per-frame render/ECS hot paths have no `dhat` coverage
> (the profiler is a process singleton). All Dim-1 allocation observations
> below are source-read, not measured — flagged per finding.

---

## Hot-Path Analysis

No bench was run this pass. Static structural review of the per-frame surfaces:

- **CPU per-frame allocations** (Dim 1): steady-state scratch reuse is healthy
  across `build_render_data`, the animation/billboard/particle/light/weather
  systems, and the `render_one_frame` drain path. The only residual per-frame
  heap churn is in the scheduler timing collection (D1-NEW-01, LOW).
- **Draw loop** (Dim 2): sort key, batch-merge, and per-draw state are all
  change-gated; instanced opaque batches collapse correctly via
  `gl_InstanceIndex`. The one structural gap is particle billboards never
  batching (D2-NEW-01, MEDIUM).
- **GPU memory** (Dim 3): BLAS budget dynamic + LRU eviction smooth; scratch
  shrink honors reserve floors; deferred-destroy honors `MAX_FRAMES_IN_FLIGHT`.
  No new pressure findings.
- **GPU passes** (Dim 5): all compute dispatches O(pixels)/O(froxels); AS
  build→read barrier present; Disney lobes gated; `inv_vp` CPU-side. No new
  pipeline inefficiencies.

---

## Findings (deduplicated, grouped by severity)

### MEDIUM

#### D2-NEW-01: Particle billboards never instance-batch; additive emitters drop free instancing
- **Severity**: MEDIUM
- **Dimension**: Draw & Instancing
- **Location**: `byroredux/src/render/particles.rs:81-144` (per-particle
  `DrawCommand` build), `byroredux/src/render/mod.rs:195-209` (transparent
  sort-key branch), `crates/renderer/src/vulkan/context/draw.rs:2034`
  (batch-merge SSBO-contiguity requirement),
  `crates/core/src/ecs/components/particle.rs:279/295/315/329/350/387/401/414/428`
  (preset `max_particles` + `dst_blend` defaults)
- **Status**: NEW
- **Description**: Every particle billboard is emitted as a `DrawCommand` with
  `alpha_blend: true` and a per-particle `sort_depth`. In `draw_sort_key`, the
  transparent branch orders by `!sort_depth` (slot 6) **before** `mesh_handle`
  (slot 8), so particles of the same emitter mesh are depth-interleaved with
  every other transparent draw. Batch-merge requires contiguous same-mesh
  entries in the sorted array (`draw.rs:2034`), so each particle — up to 256 per
  emitter (`particle.rs:279`) — becomes its own draw call. The
  `render/particles.rs:6-9` module doc claims particles collapse into one
  instanced draw; that is the opaque contract and is **false** for the
  transparent particle path. Critically, the majority of vanilla presets use
  **additive** blending (`dst_blend: 1` / `ONE`, e.g. `particle.rs:295/401/428`),
  which is order-independent and needs no depth sort at all — those could be
  mesh-sorted and instanced for free.
- **Evidence**: `draw_sort_key` transparent branch (`render/mod.rs`):
  `(rt_only, 1u8, render_layer, two_sided, src_blend, dst_blend, !sort_depth,
  pack_depth_state, mesh_handle, entity_id)` — `!sort_depth` is slot 6,
  `mesh_handle` slot 8, so depth dominates mesh in the ordering. `particles.rs`
  hard-codes `alpha_blend: true` (line 87) with `sort_depth` (line 144) for
  every particle. Verified `dst_blend: 1` is the default on 4 of 5 presets.
- **Impact**: An emitter-heavy scene (fire/smoke/sparks) emits up to N×256 draw
  calls where N×1 would suffice for the additive subset. Cost scales with
  on-screen particle count, not entity count — exactly the surface a CPU draw
  budget cannot absorb on a busy combat or torch-lit scene. No correctness
  impact (additive is order-independent, so the current per-particle draws are
  visually correct, just wasteful).
- **Related**: Sort-key design in `render/mod.rs:187`; opaque batch-merge in
  `draw.rs`. The fix is purely CPU-side ordering, no Vulkan render-pass /
  pipeline / barrier change → no speculative-Vulkan caveat applies, and it is
  unit-testable.
- **Suggested Fix**: For the additive subset (`dst_blend == ONE`,
  order-independent), sort by `mesh_handle` before `sort_depth` so same-mesh
  particles stay contiguous and the existing batch-merge collapses them into a
  single instanced indirect draw. Leave true alpha-over (`ONE_MINUS_SRC_ALPHA`,
  e.g. the smoke preset at `particle.rs:364`) on the depth-sorted path. Update
  the `particles.rs:6-9` module doc to match reality. Confidence: HIGH on the
  premise (all three code sites verified); MEDIUM on the win magnitude (no bench
  run — depends on per-scene particle density).

### LOW

#### D1-NEW-01: Scheduler `run()` allocates ~25 `String`s + a `Vec` every frame for system-timing names
- **Severity**: LOW
- **Dimension**: CPU Hot Paths
- **Location**: `crates/core/src/ecs/scheduler.rs:438,448,461,473` (collection),
  `:483-485` (acknowledging doc comment); called per frame from
  `byroredux/src/main.rs:~2212`
- **Status**: NEW
- **Description**: The scheduler `run()` allocates a fresh
  `Mutex<Vec<(String, u64)>>` and pushes `entry.system.name().to_string()` per
  system (~25 heap `String`s/frame, ~1500/s @ 60 fps) **unconditionally** — even
  when the `SchedulerSystemTimings` resource is absent, in which case the Vec is
  built and discarded (the doc comment at `:483-485` admits this).
  `System::name()` returns a `type_name`-derived `&'static str`, so the `String`
  allocations are gratuitous (a `&'static str` would do).
- **Evidence**: `entry.system.name().to_string()` at the cited lines; the
  collection is built every `run()` and only consumed if the resource exists.
- **Impact**: Small, non-compounding heap churn dominated by the existing
  per-system `Instant::now()` + `Mutex` cost. Not a leak. No `dhat` guard exists
  for this site.
- **Related**: Session-46 CPU hot-path work (#1371–#1379) did not touch the
  scheduler timing path.
- **Suggested Fix**: Store `&'static str` instead of `String` (tighten the trait
  return), or gate the entire collection on `SchedulerSystemTimings` presence
  and reuse a persistent Vec via `clear()+extend`.

#### D5-NEW-01: Stale render-pass attachment comment claims a 7th "reservoir" attachment
- **Severity**: LOW
- **Dimension**: GPU Pipeline
- **Location**: `crates/renderer/src/vulkan/context/draw.rs:641-643`
- **Status**: NEW
- **Description**: The comment above the `clear_values` array still reads
  "7 color attachments… 5 albedo, 6 reservoir, 7 depth", but the actual array
  (and the render pass) is now 6 color + depth with no reservoir attachment
  (removed by #1583 / commit `218b425b`). The `clear_values` array itself
  correctly has 7 entries (HDR + 5 G-buffer + depth) — only the comment is
  stale.
- **Evidence**: `gbuffer.rs` `struct GBuffer` has exactly 5 attachments (normal,
  motion, mesh_id, raw_indirect, albedo); `context/helpers.rs:78` confirms
  "6 color attachments + depth"; no `gb_reservoir` / `reservoir` symbol exists
  anywhere in `crates/renderer/src/`.
- **Impact**: Zero runtime cost; maintenance hazard only — a future editor could
  re-introduce a stale clear value or mis-index an attachment reference off the
  wrong comment.
- **Related**: #1583 (now stale — see below).
- **Suggested Fix**: Update the comment to "6 color attachments + depth… 5
  albedo, 6 depth." Confidence HIGH (comment-only, no Vulkan state change).

---

## Resolved / Stale Since Last Audit (verified, not re-filed)

| Prior finding | Status | Evidence |
|---|---|---|
| F1 (2026-06-14, HIGH) — indirect draw-group merge spans differing `two_sided`/depth | **RESOLVED** | `673e02e1` (#1581); `group_state` keys merge; `group_state_tests` |
| F5 (2026-06-14, MEDIUM) — `about_to_wait` two fresh `HashSet`s/frame | **RESOLVED** | #1584 persistent `in_use_*_scratch` reused via `clear()` |
| F2 / PERF1-01 — caustic per-light `exp()` (re-affirmed twice) | **RESOLVED** | `e8a7a386` (#1582); `kGauss5[25]` hoisted before per-light loop |
| F4 / PERF2-02 — BLAS build-scratch shrink on resize only | **RESOLVED** | `shrink_blas_scratch_to_fit` from `cell_loader/unload.rs:134` |

### Open GitHub issue recommended for CLOSURE

- **#1583 — "Write-only ReSTIR-DI reservoir attachment (16 B/px × 2 FIF)"**:
  **STALE**. The attachment was removed by commit `218b425b`. G-buffer is now
  6 color + depth; no `gb_reservoir` symbol survives in the renderer crate. The
  audit-performance SKILL's claim (`#1583/#1590` retired the attachment, 7→6)
  is correct; the open issue should be closed.

---

## Baselines Verified Intact (must-not-regress)

**Dimension 1 — CPU Hot Paths** (all Session-46 guards intact):
- `#1371 PackedStorage::drain_dirty_into` (`packed.rs:73`) — uses `append`,
  preserves capacity; both production callers use it; `take_dirty` only in
  tests. Guard test `drain_dirty_into_preserves_storage_capacity` present.
- `#1372 make_animation_system` — two persistent scratch Vecs, clear+extend.
- `#1374 make_billboard_system` — `last_cam` gate intact, skips loop on
  unmoved camera.
- `#1376 build_debug_ui_snapshot` — gated on `visible` (`main.rs:1517`).
- `#1379 SkinSlotPool` — `next_slot` contracts after idle-slot sweep
  (`resources.rs:866`).

**Dimension 2 — Draw & Instancing**:
- `draw_sort_key` (`render/mod.rs:187`) and CPU batch-merge (`draw.rs:2026`) key
  on the same axes; identical opaque state runs fold correctly.
- Opaque instancing via `gl_InstanceIndex` (incl. `firstInstance`),
  `triangle.vert:127` — no N-copies bug.
- Per-draw state (`cmd_set_depth_bias` / `cmd_set_cull_mode` / depth) change-gated
  against `last_*` trackers (`draw.rs:2689-2750`).
- `par_sort_unstable_by_key` `>= 2000` gate (`render/mod.rs:397`) — Bethesda
  400–1500 range correctly stays serial.
- `#1377` GT-presence hoist (`static_meshes.rs:147-149`) intact.
- Textures bindless — set 0/1 bound once per frame (`draw.rs:2474-2493`),
  correctly not a sort axis.

**Dimension 3 — GPU Memory Pressure** (all PASS):
- BLAS budget dynamic: `device_local/3` floored at `MIN_BLAS_BUDGET_BYTES`
  (256 MB), `predicates.rs:547`; mid-batch eviction at 90% every 64 builds;
  budget compares `static_blas_bytes` (excludes non-evictable skinned).
- LRU `evict_unused_blas` (`blas_static.rs:1016`) — ascending `last_used_frame`,
  oldest-first, only touches `idle >= MAX_FRAMES_IN_FLIGHT+1`.
- `shrink_*_to_fit` honor `WORKING_SET_FLOOR = 8192` + correct per-path slacks
  (16 MB BLAS / 1 MB TLAS-instance / 256 KB TLAS-scratch) with 2× hysteresis.
- Mesh pool caps fire via `check_pool_growth` (`mesh.rs:55`) — soft warn / hard
  error per upload.
- BGSM/BGEM half-eviction (oldest N/2 via insertion-order `VecDeque`) — no
  full-flush path (`asset_provider.rs:886/920/964/980`).
- `NifImportRegistry` 2048-entry LRU bounds scene count.
- Deferred-destroy = `MAX_FRAMES_IN_FLIGHT` (2) across generic / mesh / BLAS /
  texture paths — frees no in-flight memory; tick runs after both-slots fence
  wait.

**Dimension 5 — GPU Pipeline** (all PASS):
- All compute dispatches O(pixels)/O(froxels): volumetrics 160×90×128 froxel
  grid (single sun-shadow ray/froxel, and gated behind
  `VOLUMETRIC_OUTPUT_CONSUMED=false`); bloom O(pixels)/mip; SVGF/TAA/SSAO from
  screen extent.
- TLAS build-vs-refit via `decide_use_update`; **AS build→read barrier present**
  (`draw.rs:1714-1722`) plus COMPUTE→AS_BUILD_INPUT (`:1447`) and refit→TLAS
  (`:1597`). No missing-barrier HIGH.
- Disney BSDF lobes gated (`lighting.glsl:120-151`): anisotropic GGX only on
  `mat.anisotropic > 0`; sheen/subsurface only on `MAT_FLAG_PBR_BSDF`; cheap
  Lambert otherwise. No dead-lobe ALU.
- `inv_vp` computed once on CPU (`draw.rs:791`), passed via UBO; SSAO reads
  `invViewProj` from UBO. Only shader `inverse()` is the normal matrix
  (`triangle.vert:189`), gated by `INSTANCE_FLAG_NON_UNIFORM_SCALE`.
- `NUM_RESERVOIRS = 16` (`triangle.frag:1916`), per-thread storage already
  cut 320 B → 128 B (#1369 still OPEN, tracked, not re-reported).

---

## Hygiene Notes (not defects)

- Stale doc-comment at `byroredux/src/cell_loader/nif_import_registry.rs:278`
  says "default `BYRO_NIF_CACHE_MAX=0` mode" — wording only; actual default is
  2048.

---

## Prioritized Fix Order

1. **Close #1583** — issue is stale (attachment removed by `218b425b`). Zero
   work, removes a false-positive from the tracker.
2. **D5-NEW-01 (LOW)** — one-line comment fix in `draw.rs:641-643`. Trivial,
   removes a maintenance hazard while the context is fresh.
3. **D1-NEW-01 (LOW)** — scheduler timing-name allocation: switch to
   `&'static str` or gate on resource presence. Small, contained.
4. **D2-NEW-01 (MEDIUM)** — additive-particle instancing: CPU-side sort-key
   change for `dst_blend == ONE`, plus module-doc correction. Highest potential
   per-frame win of this pass on emitter-heavy scenes; bench before/after on an
   FX-dense control scene to quantify (no current bench-of-record covers it).

---

## Out of Scope (this pass)

Dimensions 4 (SSBO Sizing & Upload), 6 (Skinning & BLAS), 7 (Streaming &
Cells), 8 (NIF Parse), and 9 (Telemetry & Origin Cost) were not audited per the
`--focus 1,2,3,5` constraint. Open issue **#1369** (WRS reservoir loop) is a
Dim-1-shader / Dim-5-adjacent item that remains OPEN and tracked; it was not
re-investigated here. The 2026-06-14 report's Dim-7 findings (F6 CSG re-parse,
F7 spawn-drain) and Dim-4 finding (F8 full-buffer flush) are outside this focus
and unchanged by this pass.
