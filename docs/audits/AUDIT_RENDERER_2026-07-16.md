# Renderer Audit — 2026-07-16

**Scope**: Full 21-dimension sweep of the Vulkan renderer (`crates/renderer/`) — acceleration
structures, SSBO/ray-query correctness, GPU-struct layout, synchronization, GPU memory
lifecycle, NIFAL material translation, material table dedup, denoiser/composite, GPU
skinning, camera-relative precision, pipeline/render-pass state, command buffer recording,
TAA, caustics, water, volumetrics/bloom, Disney BSDF/soft shadows, sky/weather, tangent-space,
debug telemetry, and the Cornell-box RT harness.

**Method**: Six dimension-group sub-agents (renderer-specialist), each auditing 3-4 adjacent
dimensions against `docs/engine/shader-pipeline.md` and `docs/engine/memory-budget.md` as the
authoritative references, deduplicated against `gh issue list --repo matiaszanolli/ByroRedux`
and the ~80 prior `docs/audits/AUDIT_RENDERER_*.md` reports (most recently the 2026-07-14 and
2026-07-15 dimension-specific passes). Two of the six groups (Dim 1-3, Dim 4-7) stalled on
their first run and were re-launched with a tighter scope before completing. All six groups'
final output was independently verified against their raw transcripts before being merged
into this report — no sub-agent summary was taken on trust alone.

`cargo test -p byroredux-renderer --lib`: 379 passed, 0 failed (per the Dim 11-14 agent).

## Executive Summary

**Zero new findings across all 21 dimensions.** This is a mature, previously-audited
codebase (daily renderer audit cadence for the last two weeks), and this sweep confirms
that state holds: every load-bearing regression guard checked — AS build-flag stability,
`instance_custom_index`/SSBO lockstep, the ReSTIR-DI spatial normal-cone, BC1 punch-through
alpha, GPU-struct byte/offset pins, deferred-destroy queues (BLAS, BLAS-scratch, egui),
`AllocatorResource` teardown ordering, the NIFAL single-boundary invariant, material-table
dedup identity, SVGF firefly-clamp hoist, GPU-skinning barrier masks, the camera-relative
render-origin split, G-buffer format/mesh-ID contracts, TAA parked-camera α floor, caustic
direct-only routing, water Fresnel/IOR separation, Disney BSDF gating, tangent-space
handedness, egui/GPU-timer lifecycle, and the Cornell harness's shared-path invariant —
verified intact against the live tree.

One doc-drift observation (shader-pipeline.md's instance-flag table omitting bit 8
`INSTANCE_FLAG_DIFFUSE_ALPHA`) surfaced independently in the Dim 1-3 pass, but it dedups
directly to already-open **#1915** (also independently re-confirmed open by the Dim 11-14
agent) — not filed as new.

| Severity | Count |
|----------|-------|
| CRITICAL | 0 |
| HIGH     | 0 |
| MEDIUM   | 0 |
| LOW      | 0 (1 candidate, deduped to existing #1915) |
| **Total NEW findings** | **0** |

Pipeline areas swept with zero new defects: acceleration structures, ray-query shaders,
GPU-struct layout, sync/barriers, GPU memory lifecycle, NIFAL material translation,
material table, denoiser/composite, GPU skinning, camera-relative precision, pipeline
state/G-buffer, command buffer recording, TAA, caustics, water, volumetrics, bloom,
Disney BSDF, soft shadows, sky/weather, tangent-space, debug telemetry, Cornell harness.

## RT Pipeline Assessment

- **BLAS/TLAS correctness (Dim 1)**: geometry format, build-flag constants, `built_flags`
  VUID-03667 refit guard, and `instance_custom_index`/SSBO 24-bit contract all verified
  against the live tree. `TRIANGLE_FACING_CULL_DISABLE` is confirmed **deliberately** gated
  on `draw_cmd.two_sided` (#416) rather than applied unconditionally — the audit checklist's
  "on all instances" wording is stale prompt text, not a code issue (noted for skill
  touch-up, not filed). Deferred BLAS destruction (`pending_destroy_blas`, #a476b256) and
  deferred BLAS-scratch destruction (`pending_destroy_scratch`, #1782) both route correctly;
  the seven direct `destroy_acceleration_structure` call sites in `blas_static.rs` were
  individually confirmed to be build/copy-failure cleanup or post-compaction original-destroy
  after fence retirement — none are live-eviction sites.
- **SSBO indexing & ray queries (Dim 2)**: `instance_custom_index` (not `gl_InstanceID`)
  confirmed as the sole SSBO lookup key at every RT hit site. BC1 punch-through-alpha guard
  (`texColor.a = 1.0` when `INSTANCE_FLAG_DIFFUSE_ALPHA` clear, #ae285062) intact. ReSTIR-DI
  spatial reuse's 25° geometric-normal cone (`SPATIAL_NORMAL_COS = 0.906`, #d523b9b3) still
  gates on the packed geometric normal, reservoir still 32 B. Glass/IOR refraction (Frisvad
  basis, window-portal demote #789, `GLASS_RAY_BUDGET` documented overshoot #1438) all intact.
- **Denoiser stability (Dim 8)**: SVGF firefly-clamp hoist ahead of `hasHistory` (#48906670)
  now additionally pinned by a source-scanning regression test (#1993, closed by `02088dd9`
  since the last pass). Composite reassembly order (direct + indirect·albedo + caustic →
  volumetric → bloom → ACES) confirmed correct in `composite.frag`; caustic dual-accumulator
  (glass + water) sums to **direct** only, never SVGF-denoised indirect.

No RT-path regressions found. No new needs-RenderDoc items beyond the one already carried
from prior sweeps (see below).

## GPU-Struct & Memory Assessment

- **GPU-struct layout (Dim 3)**: `GpuInstance` (112 B), `GpuCamera` (336 B), `GpuMaterial`
  (300 B) size pins and all per-field offset assertions verified passing (25 tests green
  per the sub-agent). `struct GpuInstance` confirmed declared in exactly the 5 expected
  shader sites (`include/bindings.glsl`, `triangle.vert`, `ui.vert`, `water.vert`,
  `caustic_splat.comp`) with no drift. `DBG_*` catalog is now **19** bits (grew again since
  the last "18" count noted on 2026-07-15, via `DBG_VIZ_MOTION = 0x20000` / #1874 +
  `DBG_RESERVED_20`), still build-time lockstep-guarded — the audit skill's checklist wording
  is now stale twice over (13→18→19); flagged as a skill-doc touch-up below, not a finding.
- **Memory & resource lifecycle (Dim 5)**: `AllocatorResource` removal-before-`VulkanContext`
  drop ordering confirmed intact across the panic-unwind path (REG-08/#1640/#1477/#1406).
  Deferred-destroy tick ordering (`#418`), TLAS resize `device_wait_idle` before free
  (`#1390`), and `shrink_tlas_to_fit`/`shrink_tlas_scratch_to_fit` placement (`#1911`/
  REN-D1-01) all match `memory-budget.md` with no drift.
- **NIFAL material translation (Dim 6)**: `translate_material` confirmed to have exactly
  two production callers (`scene/nif_loader.rs`, `cell_loader/spawn.rs`); the only other
  `Material { … }` literals are the Cornell RT harness (`cornell.rs`, an intentional
  self-contained test scene, not a translation leak) and a `#[cfg(test)]` helper. `resolve_pbr`
  idempotency and the particle color-preservation guard remain test-backed.
- **Material table (Dim 7)**: over-cap handling (id 0 + warn-once + `overflow_count`
  telemetry, #797/#807/#7823eb59), raw-byte `Hash`/`Eq` lockstep with the Dim-3 struct
  invariant, and the particle color-fade quantization guard (#1795, `COLOR_FADE_STEPS = 32`)
  all confirmed intact.

No GPU-struct drift, no per-frame leaks, no lifecycle-ordering regressions found.

## Findings

No new findings at any severity. The one candidate (shader-pipeline.md's instance-flag
table stopping at bit 7, omitting bit 8 `INSTANCE_FLAG_DIFFUSE_ALPHA`) dedups to
**Existing: #1915** ("REN-D2-03: shader-pipeline.md descriptor + instance-flag tables lag
the live Set-1 layout"), independently re-confirmed open by both the Dim 1-3 and Dim 11-14
sub-agents. No action taken here beyond confirming it remains open and undupped.

### Regression guards re-confirmed across all 21 dimensions

Acceleration structures: build-flag constants, `built_flags` refit assert, `instance_custom_index`
24-bit contract, TLAS UPDATE padding guard, deferred BLAS/BLAS-scratch destruction.
Ray queries: RT gate (`sceneFlags.x > 0.5`), BC1 punch-through pin, ReSTIR-DI spatial cone,
Frisvad glass basis, window-portal demote. GPU structs: all three struct size/offset pins,
5-site `GpuInstance` lockstep, capacity constants. Sync: per-swapchain-image `render_finished`
split (548c1b69), AS-build INPUT barrier access flags (#507945d8), egui EXTERNAL dependency
(#1433), swapchain-recreate wait-idle ordering. Memory: `AllocatorResource` teardown order,
deferred-destroy tick-after-fence-wait, TLAS resize wait-idle. NIFAL: single-boundary
invariant, `resolve_pbr` idempotency, particle color preservation. Material table: over-cap
handling, byte-hash lockstep, particle fade quantization. Denoiser: firefly-clamp hoist
(+ new #1993 test), composite tone-map-after-reassembly ordering, caustic direct-only
routing. Skinning: `VERTEX_STRIDE_FLOATS` single-source, scratch-serialize barrier dual
WRITE|READ mask (#1790). Precision: raster/RT space separation, rigid+skinned rebase
(#1486), motion-vector origin correction (#1489), RT precision ceiling, DoF focus guard.
Pipeline/G-buffer: vertex-input/format lockstep, mesh-ID bit-31 contract, render-pass
EARLY/LATE symmetry, pipeline-cache header validation. Command buffer: draw-counter
independence (#1258-1260). TAA: Halton jitter fallback, parked-camera α floor (#1497),
YCoCg clamp. Caustics: named-flag macro (#1234), direct-only output. Water: Fresnel/IOR
separation, sun-direction sign convention (now resolved, was REN-DIM17-02, fixed by
#1635/#1459). Volumetrics/bloom: froxel grid constants, `VOLUMETRIC_OUTPUT_CONSUMED` gate,
bloom pre-ACES ordering. Disney BSDF: `MAT_FLAG_PBR_BSDF` sole gate, `dielectricF0FromIor`
clamp, GGX aniso degeneration, `deriveAxAy` domain clamp, sheen /PI convention. Sky/weather:
`GameTimeRes` reset guard (REN-D18-01, confirmed fixed since 2026-07-15). Tangent-space:
Bethesda bitangent/tangent swap (#786), FO4+ BSTriShape precedence (#795/#796), shared
`bitangent_sign` helper (#1516). Debug/telemetry: egui pass layout/dependency correctness
(#1433/EGUI-04), GPU-timer capability gating (#1478), `dispatches_skipped` location (#1194).
Cornell harness: shared MaterialTable round-trip, color-space rule compliance.

## Prioritized Fix Order

No correctness or safety fixes are outstanding from this sweep. Nothing to prioritize.

## Needs-RenderDoc

- **Caustic atomic-add → SHADER_READ ordering and G-buffer → compute-consumer transitions**
  (Dim 4/5, `draw.rs` post-geometry barrier block, `record_post_geometry_passes`). The
  barriers are present and match documented intent, but their GPU-visible correctness
  (actual hazard-free execution order under the driver) cannot be confirmed or refuted by
  static reading alone. Carried forward as an open observation, not a code defect claim —
  per the standing no-speculative-Vulkan-changes policy, no change is proposed.

## Skill Doc-Touchup Suggestions (non-findings, for `audit-renderer` maintainers)

Two audit-checklist wording drifts were independently flagged by sub-agents; neither is a
shipped-doc or code bug, both are text in `.claude/commands/audit-renderer`:

1. **DBG_* catalog count is stale.** The checklist says "13-bit DBG_* catalog"; the live
   count has grown to 19 (`DBG_BYPASS_POM 0x1` … `DBG_VIZ_MOTION 0x20000`, most recently via
   #1874), all still build-time lockstep-guarded (#1860). Suggest rewording to reference the
   guard mechanism rather than a fixed count.
2. **GPU-timer "reset" wording.** Dimension 20's checklist says "`cmd_reset_query_pool`
   before re-recording brackets"; the implementation uses host-side `device.reset_query_pool`
   (`VK_KHR_host_query_reset`, core in 1.2) gated on `hostQueryReset` capability — a valid,
   cheaper alternative that still satisfies the reset-before-reuse invariant. Suggest
   rewording to "reset (host-side or command-buffer)".
3. **AS-instance cull-disable wording.** Dimension 1's checklist says
   "`TRIANGLE_FACING_CULL_DISABLE` on all instances" — current (and correct, #416) behavior
   gates this per-`draw_cmd.two_sided`. Suggest rewording to reflect the conditional.

## Appendix: Sub-Agent Run Notes

- Dim 1-3 and Dim 4-7 each required one restart: their first attempts stalled mid-tool-call
  without producing a final report (confirmed via raw transcript inspection — both ended on
  a bare tool-result with no trailing assistant text, at the same ~01:56 UTC timestamp). The
  restarts completed normally with a tighter scope directive and are the results reported
  above. The other four groups (Dim 8-10, 11-14, 15-17, 18-21) completed on their first run.
- All six groups' final results were cross-checked against their raw subagent transcripts
  (`~/.claude/projects/.../subagents/agent-<id>.jsonl`) before being merged into this report,
  not taken on relayed-summary trust alone.
