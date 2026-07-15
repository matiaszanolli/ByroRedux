# Performance Audit — 2026-07-15

**Scope**: `--focus 2` (Draw-Call & Instancing Efficiency only), `--depth deep`.
Dimensions 1, 3–9 were not run this pass.

## Executive Summary

Dimension 2 (draw-call/instancing) is healthy. Both regression guards this
dimension owns — the GT-presence hoist (#1377) and the two-sided blend split's
`z_write` gate (#1804) — are verified intact, with no erosion. No MEDIUM-or-higher
findings survived scrutiny; two LOW documentation/hygiene items were found and
are safe to fold into a future doc-rot bug-bash rather than tracked as urgent.

No bench-of-record comparison is included — this run did not exercise the
engine (static analysis only, per the dimension's checklist), and ROADMAP's
bench-of-record is already flagged 613 commits stale as of Session 56.

## Hot Path Analysis

Not applicable this run — Dimension 9 (Telemetry & Camera-Relative Origin
Cost), which sources `gpu_timers`/`ScratchTelemetry` numbers, was not in
scope for `--focus 2`.

## Regression Guards Verified (not re-proposed)

- **GT-presence hoist (#1377)** — `byroredux/src/render/static_meshes.rs:156`.
  The `GlobalTransform` presence check is still the first statement in the
  static-mesh draw loop, ahead of the visibility/effects/world-bound sibling
  probes, and its binding is reused rather than re-fetched.
- **Two-sided blend split gate (#1804)** — `crates/renderer/src/vulkan/context/draw.rs:316`.
  `needs_two_sided_blend_split` still requires `z_write` alongside
  `is_blend && two_sided`, so `z_write:false` particle batches skip the dead
  FRONT-cull pass. Covered by two tests (`does_not_split_when_z_write_false`,
  `splits_when_blended_two_sided_and_z_write`).

Also confirmed sound in passing: descriptor sets and viewport/scissor bind
once per frame (not per draw); all per-batch dynamic state (pipeline, depth
bias, depth test/write/compare, cull mode) is change-gated; instancing
correctly collapses same-mesh/same-state entities into one indirect draw via
bindless per-instance texture/material indices, so texture/material variation
does not fragment batches.

## Findings

### DIM2-01: Transparent + wireframe draws can interleave pipelines across meshes (additive-blend path)
- **Severity**: LOW
- **Dimension**: Draw & Instancing
- **Location**: `byroredux/src/render/mod.rs:209-257` (`draw_sort_key`, transparent branch), `pack_depth_state` (l.50)
- **Status**: NEW
- **Description**: In the alpha-blend sort-key branch, the wireframe bit
  (packed into `pack_depth_state`) lands at sort slot 7, while for additive
  blend the mesh handle sorts at slot 6 — ahead of it. The opaque branch
  orders these correctly (depth_state before mesh). A scene with multiple
  distinct additive-blend meshes each present in both wireframe and fill
  variants would bind pipelines `fill,wire,fill,wire` instead of the optimal
  `fill,fill,wire,wire`.
- **Impact**: `NiWireframeProperty` combined with additive-blend
  `NiAlphaProperty` is effectively absent from Bethesda content — real-world
  blast radius is ~zero. No correctness impact; ordering doesn't affect the
  additive blend result.
- **Related**: #1806 (D2-NEW-05 — the wireframe-into-depth_state packing this stems from)
- **Suggested Fix**: Not worth the sort-key-width cost today. If ever
  measurable, give the transparent branch a dedicated wireframe slot ahead of
  the mesh slot, mirroring the opaque branch.

### DIM2-02: Stale sort-key documentation (9-tuple) and magic-literal sort threshold
- **Severity**: LOW
- **Dimension**: Draw & Instancing
- **Location**: `byroredux/src/render/mod.rs:205,445` (comments), `:462` (`>= 2000` literal)
- **Status**: NEW
- **Description**: Two doc comments still describe the pre-Option-B
  9-tuple sort key; `draw_sort_key` has returned a 10-tuple since (a
  third comment correctly says "10-tuple"). Separately, the
  serial/parallel sort crossover (`2000`) is an inline literal rather than a
  named constant next to its justifying benchmark table.
- **Impact**: Documentation-only. The threshold value is well-chosen — the
  in-file 7950X benchmark shows serial winning 28–35% at 400–1500 draws
  (typical FNV/Skyrim interiors) and parallel winning 14–67% at 3K–10K+ (FO4
  CSG-dense cells), with 2000 sitting at the measured tie point.
- **Related**: #934 (PERF-DC-01, original threshold tuning)
- **Suggested Fix**: Update the two stale "9-tuple" comments to "10-tuple";
  hoist `2000` into a named `const DRAW_SORT_PARALLEL_THRESHOLD` beside the
  benchmark table.

## Items Checked, Not Issues

1. Opaque sort-key clustering (`render_layer → two_sided → depth_state → mesh`, depth tiebreaker) exactly matches the batch-merge grouping key — every legal collapse is captured by simple `batches.last_mut()` extension.
2. No path emits N draws for N identical-mesh/identical-state entities; bindless per-instance texture/material data prevents batch fragmentation from texture/material variation.
3. Push-constant/state churn: all per-batch dynamic state is change-gated. The only unconditional per-item rebinds are in the water pass and the non-global-buffer per-mesh fallback, both bounded to a handful of items per cell — not on the hot static-mesh path.
4. Parallel-sort threshold (2000) correctly routes typical interiors to serial and FO4 CSG-dense exteriors to rayon, per the embedded benchmark.

## Prioritized Fix Order

Neither finding warrants urgent action:
1. DIM2-02 (5-minute doc/const cleanup) — safe filler for the next doc-rot bug-bash.
2. DIM2-01 — documented for completeness only; not worth a sort-key change given ~zero real-world blast radius.

---
Full dimension detail: see agent trace (this report already contains it in full — no separate per-dimension file was retained).
