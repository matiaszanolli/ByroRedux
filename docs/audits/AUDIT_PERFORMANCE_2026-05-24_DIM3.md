# Performance Audit — Dimension 3: Draw Call & Batching Overhead

**Date**: 2026-05-24
**Scope**: Vulkan draw-loop batching, sort key correctness, instanced-draw collapse, per-draw state changes
**Trigger**: Skyrim Riverwood radius-3 (49 cells, 12,652 entities) reportedly producing 12,277 draws/frame at ~2 FPS (~44 µs/draw)
**Investigator's note**: Read-only investigation; no source files modified.

---

## Executive Summary

The triggering measurement is misframed by an instrumentation bug, not a batching failure. Verified, in order of impact:

- **PERF-D3-NEW-03 (HIGH)** — `DebugStats::draw_call_count` is mislabelled: the field stores `draw_commands.len()` (pre-batch input) but its doc-comment and console output (`stats` command) call it "Draw calls". The 12,277 number is the **DrawCommand count** entering the batch merger, NOT the count of `cmd_draw_indexed` / `cmd_draw_indexed_indirect` calls actually submitted. The "~44 µs/draw" arithmetic is built on the wrong denominator. The real GPU call count is `batches.len()` (further compressed by multi-draw-indirect grouping into one `cmd_draw_indexed_indirect` per `(pipeline_key, render_layer)` run), and is not surfaced anywhere — neither in `DebugStats`, the bench summary, nor `byro-dbg stats`.

- **PERF-D3-INFO-01 (informational)** — The batch merger itself (`draw.rs:1574-1603`) is correct. Sort key (`render/mod.rs:160-193`) correctly clusters opaque draws on `(rt_only, alpha_blend, render_layer, two_sided, depth_state, mesh_handle, ...)`, which matches the 7-tuple batch key exactly. `first_instance` and `instance_count` are real instanced-draw arguments (`draw.rs:2222`, `draw.rs:2248`). Push constants are NOT emitted in the main raster draw loop (grep confirms zero `cmd_push_constants` calls in `draw.rs`). Bindless texture set (set 0) and scene SSBO set (set 1) are bound **once per frame** at `draw.rs:1946-1964`, not per draw. None of the items 1-6 in the audit checklist are actually broken in current code.

- **PERF-D3-NEW-04 (MEDIUM)** — Pre-population of the blend pipeline cache (`draw.rs:1721-1737`) walks **every** batch each frame and `HashMap::contains_key`s the `(src, dst, wireframe)` triple even when the cache is in steady state and has been for thousands of frames. On a Riverwood frame with N opaque + M blended batches, that's M hash lookups every frame that always hit, post-warmup. Cheap individually, but visible in CPU-bound regimes where M is large (Skyrim hair / fur / leaves all blend).

- **PERF-D3-NEW-05 (LOW)** — Off-frustum (`in_raster=false`) draws still walk through the batch-formation loop (`draw.rs:1425-1604`) and consume CPU on the model-matrix non-uniform-scale detection (`draw.rs:1438-1444`) and flag assembly before being skipped at line 1536. They emit a GpuInstance for the TLAS-only contract (#516) but the pre-skip cost is paid 12,277× in the worst case.

The Riverwood 540 ms/frame budget is real, but the bottleneck is **not** "12,277 GPU draw calls at 44 µs each". The actual culprits to look at are SVGF / TAA / SSAO / RT bandwidth (Dim 1, 2, 7) — out of this dimension's scope but worth flagging because the 12,277 figure has been driving wrong conclusions.

---

## PERF-D3-NEW-03: `DebugStats::draw_call_count` is mis-labelled — reports input to batcher, not GPU draw calls

- **Severity**: HIGH (misdiagnosis blocker, not a perf bug per se)
- **Status**: NEW
- **Dimension**: Draw Call Overhead
- **Location**: `byroredux/src/main.rs:1589`, `crates/core/src/ecs/resources.rs:224-225`, `byroredux/src/commands.rs:72`, `byroredux/src/main.rs:2041`
- **Description**: The field `DebugStats::draw_call_count` is documented as "Draw calls last frame" (resources.rs:224) and surfaced to the user as `"Draws: {n}"` in the `stats` console command (commands.rs:72) and the bench-line `draws={n}` in the bench-frames summary (main.rs:2041). But the only writer (main.rs:1589) sets it from `self.draw_commands.len()`, which is the **pre-batch DrawCommand vector** built by `build_render_data`. The actual `cmd_draw_indexed` / `cmd_draw_indexed_indirect` call count after batching + multi-draw-indirect grouping is never recorded. The Riverwood "12,277 draws/frame ~ 44 µs/draw" claim derives from this mislabelled number and is therefore unsound — at 12,277 input commands the batcher could be collapsing them into anywhere from ~5 to ~12,277 actual GPU calls and the metric can't tell us which.

- **Evidence**:
  ```rust
  // main.rs:1587-1590
  // Record draw call count for diagnostics.
  world_resource_set::<DebugStats>(&self.world, |s| {
      s.draw_call_count = self.draw_commands.len() as u32;
  });
  ```
  ```rust
  // resources.rs:224-225
  /// Draw calls last frame.
  pub draw_call_count: u32,
  ```
  Meanwhile `draw.rs:2909` computes `working_batches = batches.len()` but only uses it for the scratch-shrink heuristic — never reports it up to DebugStats. And the inner indirect grouping loop at `draw.rs:2278-2296` further compresses `batches.len()` into fewer `cmd_draw_indexed_indirect` calls (`group_size` batches per call), and that final number isn't tracked anywhere either.

- **Suggested Fix**: Either (a) rename the field to `draw_command_count` and add new `batch_count` + `indirect_call_count` fields populated from `draw.rs:2909` (`batches.len()`) and from a counter incremented inside the `while i < batches.len()` loop at `draw.rs:2097`; or (b) leave the field name as-is and have `draw_frame` return the actual GPU call count, with main.rs:1589 writing that number into the field. (a) is preferable because it preserves the existing baseline numbers in the rolling 16-audit history while exposing the new metrics. Same fix applies to the `debug-protocol` crate (`debug-protocol/src/lib.rs:100`) so byro-dbg renders all three.

- **Estimated Impact**: Zero CPU/GPU. Unblocks correct diagnosis of every future "we're too slow" investigation — the next audit dimension will know whether the bottleneck is GPU-side draw cost (need to fix batching) or CPU-side command-building cost (need to fix `build_render_data`) without resorting to a custom RenderDoc capture.

---

## PERF-D3-INFO-01: Batch merger is correct — no smoking gun in the merge key or draw loop

- **Severity**: INFO (verification, not a bug)
- **Status**: NEW
- **Dimension**: Draw Call Overhead
- **Location**: `crates/renderer/src/vulkan/context/draw.rs:1415-1604` (build), `draw.rs:2097-2296` (draw loop); `byroredux/src/render/mod.rs:160-193` (sort key)
- **Description**: Walked the audit checklist (items 1-6) against current code. All are sound.

  1. **Sort key vs batch key alignment** — `draw_sort_key` opaque arm at `render/mod.rs:180-192` clusters on `(rt_only=0, 0u8, render_layer, two_sided, 0, 0, depth_state, mesh_handle, sort_depth, entity_id)`. The batch merge key at `draw.rs:1575-1582` checks `(mesh_handle, pipeline_key, two_sided, render_layer, z_test, z_write, z_function, contiguous_ssbo)`. The sort places mesh_handle at slot 7 with render_layer + two_sided + packed depth_state as the higher-priority clustering axes, so any two opaque DrawCommands that the batch key would merge are also adjacent in the sorted vec. The only sort-key axis the batch key doesn't read is `entity_id` (tiebreaker only, doesn't fragment batches) and `sort_depth` (slot 8 — within a same-mesh cluster, so doesn't fragment either). The blended-arm sort at `render/mod.rs:167-178` is correct for transparent-back-to-front-per-state but instancing is irrelevant for blended (per the sort-key doc comment).

  2. **Per-draw push constants** — `grep -rn cmd_push_constants /mnt/data/src/gamebyro-redux/crates/renderer/src/` returns only `water.rs:471` (one-shot per water plane, not per main-pass draw) and `skin_compute.rs:550, :869` (compute dispatch, not graphics). The main raster draw loop emits **zero** push constants per draw or per batch. Per-instance data flows through the `GpuInstance` SSBO indexed by `gl_InstanceIndex`.

  3. **Per-draw descriptor binds** — `draw.rs:1946-1953` binds the bindless texture set (set 0) once per frame. `draw.rs:1957-1964` binds the scene SSBO set (set 1) once per frame. No `cmd_bind_descriptor_sets` call exists inside the `while i < batches.len()` loop. Verified by walking the entire loop body at `draw.rs:2097-2310`.

  4. **`first_instance` / `instance_count` usage** — `draw.rs:2219-2226` (and the global-fallback at 2245-2252) passes `batch.instance_count` and `batch.first_instance` to `cmd_draw_indexed`. The indirect path at `draw.rs:2289-2295` uses `cmd_draw_indexed_indirect` reading per-batch records pre-built at `draw.rs:1702-1708` — each record carries the real `instance_count`. Instancing is wired correctly.

  5. **Sort key efficiency** — 10-tuple of `(u8, u8, u8, u8, u32, u32, u32, u32, u32, u32)` = 28 bytes. Sort cost was already audited in PERF-DC-01 / PERF-DC-03 (2026-05-04_DIM5) and is well-characterised; the serial-vs-parallel threshold at 2000 commands is correctly set per `render/mod.rs:339-343` with empirical benches in the comment.

  6. **Texture/pipeline bind frequency** — Pipeline binds at `draw.rs:2127-2128` only fire on `pipeline_key` change (4 possible values: Opaque×wireframe × Blended×wireframe × N (src,dst) combos, in practice ~3-5 distinct keys per frame). Texture binds are bindless — set 0 holds ~all textures via VK_EXT_descriptor_indexing, bound once per frame.

- **Estimated Impact**: None — informational, but corrects the audit-trigger's premise.

---

## PERF-D3-NEW-04: Blend-pipeline cache pre-population walks every batch every frame, with no steady-state short-circuit

- **Severity**: MEDIUM
- **Status**: NEW
- **Dimension**: Draw Call Overhead
- **Location**: `crates/renderer/src/vulkan/context/draw.rs:1721-1737`
- **Description**: The pre-loop at line 1721 iterates every batch and for each `PipelineKey::Blended { src, dst, wireframe }` does a `HashMap::contains_key((src, dst, wireframe))`. After the first few cell-load frames every (src, dst, wireframe) combo is cached and `contains_key` always returns `true`. On Riverwood with M blended batches per frame (likely a few hundred — Skyrim hair, leaves, banners, smoke), that's M hash lookups per frame producing zero work in steady state. The fix is to either (a) shortcut on `self.blend_pipeline_cache.len() == seen_unique_keys` after the first iteration, or (b) hoist the pre-population to be triggered by cell-load events instead of per-frame.

- **Evidence**:
  ```rust
  // draw.rs:1721-1737
  for batch in &batches {
      if let PipelineKey::Blended { src, dst, wireframe } = batch.pipeline_key {
          let wireframe = wireframe && self.device_caps.fill_mode_non_solid_supported;
          if !self.blend_pipeline_cache.contains_key(&(src, dst, wireframe)) {
              if let Err(e) = self.get_or_create_blend_pipeline(src, dst, wireframe) {
                  log::error!(...);
              }
          }
      }
  }
  ```
  The loop runs unconditionally regardless of whether new (src, dst) combos have appeared this frame.

- **Suggested Fix**: Track the live (src, dst, wireframe) triples seen in `build_render_data` (or in `collect_static_mesh_draws`) into a small `HashSet`. Only run the pre-population walk when the seen-set differs from the cached-set. Alternatively, hoist the pre-population to `cell_loader::spawn` — every blend combo is determined by NIF/BGSM properties known at import time. ~30 LOC for option (a).

- **Estimated Impact**: O(M) hash lookups per frame eliminated, where M = blended batch count. At Riverwood scale (estimated 300-500 blended batches given Skyrim's heavy use of NiAlphaProperty for hair/foliage/banners), this saves perhaps 50-100 µs/frame. Not the headline win, but free.

---

## PERF-D3-NEW-05: Off-frustum draws pay full per-command build cost before being skipped

- **Severity**: LOW
- **Status**: NEW
- **Dimension**: Draw Call Overhead
- **Location**: `crates/renderer/src/vulkan/context/draw.rs:1425-1538`
- **Description**: Every `DrawCommand` — including `in_raster=false` (frustum-culled) and `is_water=true` — runs through the full per-command processing in the build loop:

  - mesh registry lookup (line 1426)
  - model matrix non-uniform-scale detection: 3 dot products + 2 abs subtracts (line 1438-1444)
  - flag assembly (lines 1466-1494)
  - GpuInstance struct construction + push to vec (lines 1501-1520)
  - is_caustic_source check (line 1474)

  Only after all that does the `if !draw_cmd.in_raster || draw_cmd.is_water { continue; }` at line 1536 skip batch formation. For frustum-culled draws this work IS required by the #516 RT-only contract (the GpuInstance entry must exist for TLAS-hit shaders) — that's fine. But the order matters: the non-uniform-scale check + flag bits could be skipped, since the off-frustum entry's flags are never read by the rasterizer (only the model matrix is needed by the RT path for TLAS instance positioning).

  At Riverwood scale, the sort key places `rt_only=1` draws at the END (per `render/mod.rs:165-178`), but the build loop walks the **sorted** order, so the off-frustum tail is the last few thousand iterations. Roughly: if the frustum kept 5000 of 12,277 draws in-raster, the other ~7,000 pay the full build cost.

- **Evidence**:
  ```rust
  // draw.rs:1425-1538 — full build per command, skip only at line 1536
  for draw_cmd in draw_commands {
      let Some(mesh) = self.mesh_registry.get(draw_cmd.mesh_handle) else { continue; };
      // ... model matrix dot products, flag assembly, GpuInstance push ...
      gpu_instances.push(GpuInstance { ... });
      if !draw_cmd.in_raster || draw_cmd.is_water {
          continue;  // <-- skipped here, but build cost paid
      }
      // batch formation below
  }
  ```

- **Suggested Fix**: Split the loop into two phases: (1) construct GpuInstance for every draw (the SSBO requirement) but with a minimal model-matrix-only payload for `in_raster=false`; (2) only run flag assembly + non-uniform-scale check for in-raster draws. Alternatively, since the `rt_only` draws cluster at the end of the sorted list, the loop can bail out of the full-build path once it crosses the rt_only=1 boundary and switch to a thin GpuInstance-only path. ~20 LOC.

- **Estimated Impact**: Saves the per-command CPU cost on (12,277 − in_raster_count) draws per frame. At ~50-100 ns per skipped non-uniform-scale check + flag assembly, on ~7,000 off-frustum draws, ~350-700 µs/frame. Small absolute win, but it scales with off-screen radius — for radius-5 grids it becomes more visible.

---

## Non-findings (verified-clean checks)

These were on the audit checklist but the current code is correct:

- **Sort-key includes an unstable field**: No. The 10-tuple is fully deterministic per DrawCommand, with `entity_id` as the final tiebreaker (`render/mod.rs:154-159`).
- **`cmd_bind_descriptor_sets` per draw**: No. Bound twice per frame (sets 0 + 1), zero rebinds in the batch loop.
- **`cmd_bind_pipeline` per draw**: No. Bound on `pipeline_key` change (`draw.rs:2104-2130`); typical Skyrim frame has 3-6 pipeline binds (Opaque, Opaque-wireframe rare, ~3 blend src/dst combos, water).
- **`cmd_push_constants` per draw**: No. Zero in the main raster pass.
- **`instance_count` always 1 (broken instancing)**: No. `batch.instance_count` is the merged count and is passed correctly to both `cmd_draw_indexed` and the indirect path.
- **First-instance arithmetic broken**: No. `first_instance` increments correctly via the contiguity check at `draw.rs:1582`.
- **Sort-key complexity**: No. 28-byte fixed-stride key, byte-comparable, cache-friendly (already audited in PERF-DC-03).

---

## Cross-reference

- `feedback_audit_findings.md` — 5 of the audit's 6 checklist items were demonstrably-not-broken in current code. The triggering premise is a metric-reporting bug, not a batching bug. **The 12,277 number is the wrong denominator.** The audit-finding-hygiene memory applies precisely here.
- PERF-D3-NEW-01 (existing, 2026-05-19 audit) — already identified `build_render_data` as the CPU bottleneck. PERF-D3-NEW-05 above is a small contributor to the same overall fix space (CPU-side per-command cost) but separable.
- PERF-DC-01 / PERF-DC-03 (2026-05-04_DIM5) — already covered sort-cost characterisation. No regression.
- M52 / GPU-driven rendering (referenced in PERF-D3-NEW-01) is the structural fix when CPU-side per-command cost becomes the binding constraint.

---

## Recommendation

1. **File PERF-D3-NEW-03 as P1**. The mislabelled metric has been actively misleading the user's diagnosis for at least one session. Fix is a ~30 LOC patch across `DebugStats`, `main.rs`, `commands.rs`, `debug-protocol`, and the bench summary format string. Without it, every future "we're too slow" investigation starts from a wrong number.
2. **File PERF-D3-NEW-04 as P2**. Free win, no risk, ~30 LOC.
3. **File PERF-D3-NEW-05 as P3**. Small absolute saving, but the right shape for a refactor that will pay off when exterior radius grows.
4. **Investigate the actual 540 ms/frame Riverwood frame in Dim 1 / 2 / 7**. Before assuming the bottleneck is draw count, capture the bench-line `brd_ms / draw_ms / fence_ms / tlas_ms / ssbo_ms / cmd_ms / submit_ms / gpu_skin_disp_ms / gpu_blas_refit_ms / gpu_taa_ms` breakdown and look at the largest line. The instrumentation at `main.rs:2020-2042` exists; the user should run `--bench-frames 300 --bench-hold` on the Riverwood scene and post the bench summary. The 44 µs/draw arithmetic should be considered void.
