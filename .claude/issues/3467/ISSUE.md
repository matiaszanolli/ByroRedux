# #3467 — PERF-2026-08-27b-02: the resumable geometry rebuild's 64 MiB chunk is one blocking staged copy per frame, and no timer can measure the slice its own doc says needs tuning

**Labels**: medium, performance, renderer, vulkan, bug
**Filed**: 2026-08-27 from `docs/audits/AUDIT_PERFORMANCE_2026-08-27b.md`
**HEAD at audit**: `969d81c8`

---

**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-08-27b.md` — finding `PERF-2026-08-27b-02`
**Severity**: MEDIUM · **Dimension**: GPU Pipeline & Pass Efficiency / Telemetry
**Location**: `crates/renderer/src/mesh.rs:44-55` (the constant), `:1338-1428` (`advance_geometry_rebuild`), `crates/renderer/src/vulkan/buffer.rs:1529-1580` (`copy_bytes_range`)

## Description

`#3298`'s stated goal is to convert one multi-hundred-ms atomic stall into "several bounded per-frame slices". The slice is bounded **in bytes, not in time**, and the byte budget chosen — 64 MiB — is close to a whole frame's worth of host->device transfer, paid **synchronously** on the render thread. So the per-frame slice is still a dropped frame; there are just several of them instead of one long one. The constant's own doc admits it is untuned, and the measurement it defers to does not exist.

## Evidence

The constant is explicitly provisional (`crates/renderer/src/mesh.rs:44-48`):

```
/// Per-`advance_geometry_rebuild`-call byte budget for a resumable global
/// geometry SSBO copy (#3298). Chosen conservatively pending live
/// `grid-cross` tuning against real FO4/Skyrim/FNV data
```

Each call is one full synchronous round trip (`crates/renderer/src/vulkan/buffer.rs:1543-1571`): a `StagingPool::acquire`, a `copy_from_slice` of the whole chunk into mapped `CpuToGpu` memory, then `with_one_time_commands(...)` — which allocates a command buffer, submits, and **waits on a fence** before returning:

```rust
let (staging_buffer, staging_alloc) = staging_pool.acquire(size)?;
...
staging.mapped_slice_mut()?[..bytes.len()].copy_from_slice(bytes);
...
with_one_time_commands(device, queue, command_pool, |cmd| {
    unsafe { device.cmd_copy_buffer(cmd, staging.buffer, self.buffer, &[copy_region]); }
    Ok(())
})?;
```

`advance_geometry_rebuild` performs exactly one such call per invocation (`crates/renderer/src/mesh.rs:1362-1430`), and the frame driver calls it every frame while a rebuild is live (`byroredux/src/app_frame.rs:205-215`).

Derived cost, stated as derivation and not as measurement: 64 MiB of `copy_from_slice` into write-combined host-visible memory plus 64 MiB across PCIe with a full fence wait is order **~10 ms** on this machine's PCIe 4.0 x16 link — i.e. roughly the entire 16.7 ms budget at 60 Hz, and more than half the 23.5 ms the ROADMAP bench-of-record records for the heaviest scene (MedTek, native TAA).

Two independent things make that estimate impossible to replace with a number:

1. **No GPU timer bracket exists for it.** `crates/renderer/src/vulkan/gpu_timers.rs` exposes `cmd_*_start`/`_end` pairs for skin dispatch, BLAS refit, TLAS build, main render, TAA, SVGF, SSAO, bloom, composite, cluster cull, caustic splat, volumetrics, upscale and presentation. `grep -n geometry_rebuild crates/renderer/src/vulkan/gpu_timers.rs` returns nothing — and it could not cover it anyway, since the copy is submitted on its own one-time command buffer outside `draw_frame`'s recording.
2. **No CPU phase isolates it either.** The rebuild call sits inside `render_one_frame`'s `rof_pre_t0` bracket (`byroredux/src/app_frame.rs:55`, closed at `:393`), which also contains `build_render_data`, material interning, the UI tick and the debug-UI snapshot. `cpu_ms:` (`byroredux/src/systems/debug.rs:98-112`) therefore reports it only as part of `rof_pre_draw`.

## Impact

On an exterior traversal that grows the pool past a few hundred MB, a rebuild spans several frames and each of them takes a full-frame CPU stall — visible as a short stutter burst rather than one long hitch, which is the improvement `#3298` intended, but not the "bounded slice" the doc claims. The compounding problem is that the constant cannot be tuned: ROADMAP's bench matrix has no exterior scene *and* no timer resolves the phase, so there is no path from "chosen conservatively pending tuning" to a tuned value.

## Related

`#3298`, `#3372`; `AUDIT_PERFORMANCE_2026-08-27.md`'s Dim-7 observation that no exterior scene is in the bench matrix; `#2041`/`PERF-D9-02` (the batched timer read this would extend); #3463 (the VRAM-ledger sibling of the same code).

**Cross-reference #3443** (`REN-2026-08-27-D5-01`, the concurrent `/audit-renderer` HIGH on `mesh.rs:1244-1288` routing around `GEOMETRY_REBUILD_IDLE_THRESHOLD_BYTES`). This finding is deliberately scoped as the pacing/telemetry consequence that **survives** that fix: restoring the idle-threshold gate changes *when* the chunked path is entered, not the size or synchronicity of each chunk, and adds no timer.

## Suggested Fix

Two independent, small steps, in this order.

(a) Bracket `advance_geometry_rebuild` with a plain `Instant::now()`/`elapsed()` pair accumulated into the existing `FrameTimings` alongside `ssbo_build_ns` — the same shape `UnloadPhaseTimings` already uses, and enough to answer the question without touching the query pool.

(b) Once a number exists, re-pick `GEOMETRY_REBUILD_CHUNK_BYTES` against it (a value near 8-16 MiB would put the slice inside a frame's slack rather than consuming the frame), and add an exterior `GridCross` scene to `scripts/fsr-bench-matrix.sh` so the choice is regression-gated.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other `copy_bytes_range` / `with_one_time_commands` callers on a per-frame path)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix
