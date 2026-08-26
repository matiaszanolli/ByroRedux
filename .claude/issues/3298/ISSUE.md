# EX-07: make MeshRegistry::rebuild_geometry_ssbo yield mid-rebuild instead of one atomic Vulkan call

Plan: EX-07 remainder (parent #2376).

## Problem
`docs/engine/exterior-readiness-plan.md`'s 2026-08-23 correction to the EX-06/07
narrative found that the previously-claimed active bottleneck ("aggregate
cooperative apply pacing") doesn't hold up — `advance_streaming_apply` and
`LodWorkBudget` already share a wall-clock deadline across every streaming
phase and every distant-LOD provider, predating the claim by weeks.

What's still genuinely true and unfixed: `MeshRegistry::rebuild_geometry_ssbo`
(`crates/renderer/src/mesh.rs:1004`, calling `build_geometry_ssbo`,
`mesh.rs:921`) is a single atomic Vulkan call with **zero internal yield
capability**. The only surrounding logic
(`WorldStreamingState::geometry_batch_in_progress`, `crates/renderer` /
`streaming.rs:719-729`, consulted at the sole call site
`byroredux/src/app_frame.rs:168-182`) decides only whether the whole call
fires *this frame* — never whether it can pause partway through what can be a
600+ MiB copy. This is the documented cause of the 1.50 s worst-frame figure
in the FO4 boundary-crossing benchmark.

## Why this is separate from the rest of EX-06/07
Everything else EX-06/07 originally worried about (per-cell p50/p95/max
telemetry, NIF finalization / static placement / terrain-water-precombine /
texture-mesh-upload / BLAS-build / LOD budgeting) is already deadline-bounded
and instrumented. This is the one remaining atomic unit.

## Suggested approach
Split the geometry SSBO rebuild into resumable chunks (analogous to the
FO4 precombine-spawn resumable-per-hash pattern already used elsewhere in
this same investigation) so a partial copy can yield back to the frame loop
instead of blocking it whole. The plan doc explicitly flags this as **real,
high-risk Vulkan work**: a directly analogous earlier attempt (yielding CPU
work per mesh during BLAS submission) regressed throughput 4.5x and hit the
300s hard timeout before being reverted. Any change here needs live
`grid-cross` validation against real FNV/Skyrim/FO4 data (see
`docs/smoke-tests/`), not a speculative code change from source reading
alone — per this project's own "no speculative Vulkan fixes" convention.

## Acceptance
- `rebuild_geometry_ssbo` can pause and resume mid-rebuild without
  correctness regressions (draw counts / entity counts / TLAS instance
  counts unchanged vs. today's atomic rebuild).
- The FO4 `grid-cross` boundary benchmark's worst-frame figure drops
  measurably below the current ~1.50 s, with no regression to the
  already-fixed unload/apply/dispatch maxima.
- Validated live against at least FO4 Commonwealth (the worldspace where
  this was measured) before merging.

## Related
- Parent epic: #2376 (EX-06/07).
- `docs/engine/exterior-readiness-plan.md` §EX-06/07 "Correction (2026-08-23)".
- `feedback_speculative_vulkan_fixes` project convention: don't ship
  render-pass/pipeline/barrier changes whose failure modes are invisible to
  `cargo test` — RenderDoc or revert, not speculation.

