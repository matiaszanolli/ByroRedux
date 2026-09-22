# CONC-D2-2026-09-21-01: The ground-cover counter readback — and three older readbacks — have no device→host memory dependency; the project's newer readbacks all emit one

**Labels**: medium, sync, renderer, vulkan, terrain-exterior, bug

Filed via /audit-publish from docs/audits/AUDIT_CONCURRENCY_2026-09-21.md.

**Severity**: MEDIUM. The spec-violation floor is HIGH. It is graded MEDIUM per the #4181 / #4293 precedent on this same buffer: the consequence is diagnostic/telemetry only, and nothing is observable on the dev config. · **Dimension**: 2 — Compute → AS → Fragment Chains (ground-cover scatter → publish, including the counter readback)
**Location**:
- `crates/renderer/src/vulkan/groundcover.rs:1640-1661` (`record_scatter`): the publish barrier, then `cmd_copy_buffer` into `counter_readback[frame]`, with no `→ HOST` edge after it.
- `groundcover.rs:1352-1372` (`harvest`): invalidates and reads after the slot's fence.
- Siblings with the same gap:
  - `crates/renderer/src/vulkan/context/screenshot.rs:229-260` (`screenshot_record_copy`): `cmd_copy_image_to_buffer`, then a restore to `PRESENT_SRC_KHR` whose dst stage is `NONE`.
  - `crates/renderer/src/vulkan/context/depth_capture.rs:245-262` (`depth_capture_record_copy`): copy, then a restore whose dst is `EARLY_FRAGMENT_TESTS | FRAGMENT_SHADER | COMPUTE_SHADER`.
  - `crates/renderer/shaders/presentation.frag:195-198`: the image-health `atomicAdd`s. The presentation pass is recorded *after* the only FRAGMENT→HOST barrier (`context/draw.rs:1984-1994`), so that barrier does not cover them.

**Status**: NEW. Related to closed #2740, but **not a regression of it**:
- #2740's fix (51c725351) was documentation-only by design. The issue said "Do not blind-fix" and deferred any code change to a `BYRO_VALIDATION=1` run, so no device→host barrier ever existed at these sites to regress.
- `git log -S HOST_READ -- crates/renderer/src` shows every production `HOST_READ` edge being *added* (cluster telemetry 9c805cd79, ray probe 8e7582ed4, combustion 2325c1de4) and none removed.
- `groundcover.rs`, `screenshot.rs`, `depth_capture.rs` and `resources.rs` have never contained a `PipelineStageFlags::HOST` edge, and the ground-cover pipeline (637b65264, 2026-09-06) postdates #2740 entirely.

**Verified against**: HEAD `f97775ca8`.

## Description

- The Vulkan spec's note on waiting for fences says that signalling a fence and waiting on the host does not guarantee that device writes are visible to the host: a fence's memory dependency covers only device access, so a memory barrier or other memory dependency is required.
  - Making device writes available to the host needs a memory dependency with `HOST_READ` in its destination access scope. That performs the device→host domain operation.
  - The host-side `invalidate_if_needed` (#2752), which both `harvest` and `collect_image_health` already call, is the other half.
- The project's newer readbacks follow the rule:
  - cluster telemetry, `compute.rs:362-371`;
  - the combustion surface-light readback, `volumetrics.rs:1373-1382`, whose comment reads "the barrier supplies the device->host memory dependency that the fence's device-only access scope does not create by itself";
  - the selected-ray-probe record, `context/draw.rs:1984-1994`, a global `FRAGMENT_SHADER/SHADER_WRITE → HOST/HOST_READ` barrier;
  - the sky-filter test, `sky_cube/filter/tests.rs`: `TRANSFER_WRITE → HOST_READ` before its fence wait.
- The four readbacks above do not. The last barrier on the ground-cover counters (#4181's widened publish edge) is `COMPUTE_SHADER → DRAW_INDIRECT | VERTEX_SHADER | TRANSFER`. It orders the copy after the scatter but says nothing about the host.

## Evidence

- `grep -rn HOST_READ crates/renderer/src` finds exactly three production sites (`compute.rs:362`, `volumetrics.rs:1381`, `context/draw.rs:1993`). Every other hit is in a test.
- No `PipelineStageFlags::HOST` destination follows any `TRANSFER` copy in production code.
- The remaining `create_host_visible` users (`exposure_meter.rs`, `groundcover_bench.rs`, `water.rs`, `sky_cube.rs` and the per-pass parameter UBOs) are host→device uploads, not readbacks, so these four are the complete set of readbacks without the edge.

## Impact

Stale reads are possible on a `GpuToCpu` memory type that is `HOST_CACHED` and not coherent, or on a driver that does not flush at submit end. Affected data:
- `GroundCoverStats`: blade counts, the §11.3 density histogram and extrema. EXAL ground-cover tuning reads these directly.
- The image-health smoke gate. A stale read turns a NaN gate into one that passes anything.
- Golden-frame screenshots.
- Depth captures.

**Trigger conditions**: every frame with ground cover active, and every image-health / screenshot / depth-capture readback, on such a memory type.

**Verification path**: no validation-layer signal exists for this class. Syncval does not model host reads of mapped memory, which is why #2740's "run `BYRO_VALIDATION=1` first" precondition could never be met. The evidence is the spec text plus the in-tree precedent above. Only a non-coherent readback device can show the failure empirically.

## Related

- #2740 (closed, docs-only; see Status), #2752 (the host-side invalidate half), #4181 and #4293 (the earlier ground-cover readback edges).
- #4182 (closed) concerns the *opposite* direction and does not settle this. Host writes flushed before `vkQueueSubmit` are covered by the submit's implicit host-write ordering, which is why the HOST→device barriers it discusses are unneeded. The device→host direction has no such implicit guarantee.
- CONC-D1-2026-09-21-01 (#4601): the ground-cover `prepare` that runs `harvest` is itself an unlisted rider on the all-slots fence wait.

## Suggested Fix

- Add one global `memory_barrier(TRANSFER | FRAGMENT_SHADER, TRANSFER_WRITE | SHADER_WRITE → HOST, HOST_READ)` as the last command before `end_command_buffer`, which covers all four sites. Alternatively, add per-site edges that mirror `volumetrics.rs:1373-1382`. Either change is purely additive.
- Add a pin that every `create_host_readback` consumer's writer is followed by a `HOST_READ` edge.

Source: docs/audits/AUDIT_CONCURRENCY_2026-09-21.md (CONC-D2-2026-09-21-01)

## Completeness Checks
- [ ] **SIBLING**: every `create_host_readback` / `MemoryLocation::GpuToCpu` buffer (image health, screenshot, depth capture, ground-cover counters, cluster telemetry, ray probe, RT-LOD telemetry, combustion) is either covered by the new edge or already has one
- [ ] **TESTS**: a source-scan pin fails if the `HOST_READ` edge is removed, or recorded before the last readback writer (the presentation pass, the screenshot copy)
