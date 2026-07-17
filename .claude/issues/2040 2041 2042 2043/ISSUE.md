# Issues 2040, 2041, 2042, 2043

All four are from `docs/audits/AUDIT_PERFORMANCE_2026-07-16.md`, Dimension "Telemetry & Origin Cost". All LOW severity, renderer domain.

## #2040 — PERF-D9-01 (LOW): GPU timer brackets use TOP_OF_PIPE start, risking pass-cost misattribution
**Location**: `crates/renderer/src/vulkan/gpu_timers.rs:355-760`

Timer brackets write timestamps with `vk::PipelineStageFlags::TOP_OF_PIPE` as the start stage, so a
bracket's reported ms can absorb queue-wait from prior in-flight work — per-bracket timings are an
upper bound and must not be summed to a "total GPU ms" without caveat. Diagnostic-accuracy-only.

**Fix**: Document the caveat prominently near `GpuTimerSnapshot`'s definition.

## #2041 — PERF-D9-02 (LOW): GPU timer readback issues up to 12 separate driver round-trips per frame
**Location**: `crates/renderer/src/vulkan/gpu_timers.rs:245-345`

`read_and_reset` reads brackets individually per `active_bits` flag (up to 12 `read_bracket` calls)
rather than one batched `get_query_pool_results(pool, 0, 24, ...)` call. Minor host-side overhead only.

**Fix**: Single batched `get_query_pool_results` read, handling the "unwritten queries never become
available under WAIT" caveat already documented in the code for a full-pool read.

## #2042 — PERF-D9-03 (LOW, doc-only): ScratchTelemetry doc says 5 today; producer now emits 9 rows
**Location**: `crates/core/src/ecs/resources/mod.rs:421-426` vs `crates/renderer/src/vulkan/context/mod.rs:2785-2864`

`ScratchTelemetry`'s doc comment says `rows` "stabilises at the count of registered scratches (5
today)"; the producer now emits 9 rows. Pure doc-rot — the "bounded, reused Vec" invariant still holds.

**Fix**: Update the doc comment to reflect current row count, or drop the specific number ("one row
per registered scratch, currently N — see `fill_scratch_telemetry`").

## #2043 — PERF-D9-04 (LOW): Render origin is snapped twice per frame from independently-passed camera_pos
**Location**: `byroredux/src/render/camera.rs:160`, `crates/renderer/src/vulkan/context/draw.rs:2583-2584`

Both call sites independently call `scene_buffer::snap_render_origin` on separately-passed
camera_pos/cam_pos values. No measurable CPU cost — the risk is fragility: a future refactor that
jitters one call site's input (e.g. TAA) without the other could desync the two origins by one
cell-width at the boundary.

**Fix**: Compute `snap_render_origin` once per frame and thread the single result through both
consumers, removing the convention-only "same un-jittered camera_pos" invariant.

## Domain classification
All four: renderer (`byroredux-renderer`). #2042 also touches a doc comment in `byroredux-core`.
#2043 also touches `byroredux/src/render/camera.rs` (binary crate).
