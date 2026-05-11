# AUDIT_RENDERER 2026-05-11 — Dimension 8 (Acceleration Structures)

Focused re-audit of Dim 8 (RT BLAS / TLAS lifecycle) following the
2026-05-09 sweep that closed REN-D8-NEW-01 through NEW-14 plus the
related REN-D12-NEW-01. Scope: `crates/renderer/src/vulkan/acceleration.rs`
and its TLAS callers in `crates/renderer/src/vulkan/context/draw.rs`,
`context/resources.rs`, `skin_compute.rs`, `scene_buffer.rs`, and
`byroredux/src/render.rs` (instance enumeration).

## Executive Summary

| Severity | Count |
| --- | --- |
| CRITICAL | 0 |
| HIGH     | 0 |
| MEDIUM   | 0 |
| LOW      | 0 |
| INFO     | 0 |

**Total new findings: 0.** All four recently-shipped regression anchors
PASS; no speculative findings filed per the
`feedback_speculative_vulkan_fixes.md` memory (RenderDoc or revert,
not speculation).

## RT Pipeline Assessment

The Dim 8 surface (BLAS / TLAS / scratch / refit) was hardened
substantially across the May 7–10 audit cycle. The remaining areas
that previous audits flagged as "would benefit from runtime
validation" (scratch-buffer race windows, post-resize TLAS aliasing,
glass-loop BLAS interaction) remain out of static-analysis scope and
need RenderDoc / live-engine verification.

## Regression Check

All four anchors from the recent shipped work still hold:

- **#914 (REN-D8-NEW-04)** — `debug_assert_eq!(tlas.last_blas_addresses.len(),
  instance_count as usize, ...)` after the `mem::swap` in
  `build_tlas`. PASS at `acceleration.rs` around the LRU eviction
  block (post-swap invariant assertion retained).
- **#915 (REN-D8-NEW-05)** — `evict_unused_blas` is now the first
  statement inside `build_blas`, mirroring the batched-path
  pre-batch eviction. PASS.
- **#907 (REN-D12-NEW-01)** — `BlasEntry.built_vertex_count` and
  `built_index_count` are present at the struct definition (around
  acceleration.rs:39+), populated at all three BUILD sites (static,
  batched via `PreparedBlas` + `compact_accels`, and skinned), and
  `refit_skinned_blas` calls `validate_refit_counts` before its
  mutable borrow. PASS.
- **#642 / #644** — `record_scratch_serialize_barrier` still brackets
  the batched build dispatch in `build_blas_batch` and is called
  between every consecutive `cmd_build_acceleration_structures` pair
  that shares `blas_scratch_buffer`. PASS.

## Confirmed Sanity Checks (no issue)

- TLAS `in_tlas` filter — `draw.rs` builds the instance list by
  skipping `DrawCommand`s whose `in_tlas == false`; per-`DrawCommand`
  predicate, respects `last_used_frame` updates.
- `instance_buffers` and `indirect_buffers` are allocated with
  `MemoryLocation::CpuToGpu` (host-visible, host-coherent) — matches
  the per-frame write pattern. No staging copy required, no
  inter-frame flush gap.

## Items Already Filed / Closed

The audit re-encountered these prior REN-D8 findings; all were
already closed by the cited fix:

- **REN-D8-NEW-04** — closed by #914 (5044964).
- **REN-D8-NEW-05** — closed by #915 (eecb1b5).
- **REN-D12-NEW-01** — closed by #907 (3753ed8).
- **REN-D8-NEW-07** / **REN-D8-NEW-09** — scratch-barrier
  invariants closed by #642 / #644.
- **REN-D8-NEW-12** — `frame_counter` shared across TLAS slots
  classified as cosmetic in the prior audit; still cosmetic.

## Prioritized Fix Order

Nothing to prioritize — zero new findings.

## Caveats

The audit pass was time-boxed and did not exhaustively traverse:

- Skinned-BLAS first-sight + UPDATE-mode geometry-count invariant
  (covered by #907's `validate_refit_counts`, but the full draw.rs
  loop ordering wasn't re-walked).
- Transform matrix conversion (column-major `[f32; 16]` → 3×4
  row-major `VkTransformMatrixKHR`) still lacks a unit-test pin per
  REN-D8-NEW-11 (open low; not re-prosecuted).
- TLAS `padded_count` over-allocation trade-off per REN-D8-NEW-10
  (open low; documented).

These are existing open lows from the 2026-05-09 sweep, not new
findings.
