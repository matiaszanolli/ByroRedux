# bug, renderer, high, vulkan

## REN-D2-NEW-01: water.frag fires RT ray queries with no RT-capability gate; binding 2 (TLAS) absent on non-RT hardware

**Severity**: HIGH
**Dimension**: Ray Queries (descriptor/binding plumbing)
**Source audit**: `docs/audits/AUDIT_RENDERER_2026-06-14.md`
**Status**: NEW

> Folds in **REN-D15-NEW-01** (LOW) — the same root cause, plus the RT-hardware first-per-slot / TLAS-failure stale-TLAS sub-case. Ship the fixes together.

## Description
`triangle.frag` guards *every* ray query behind `rtEnabled = sceneFlags.x > 0.5`, uploaded as `0.0` whenever `device_caps.ray_query_supported == false`. That is exactly why `build_scene_descriptor_bindings(rt_enabled=false)` is allowed to **omit binding 2 entirely** and `validate_set_layout` lists `[2]` in `optional_bindings` for the no-RT case — triangle.frag's binding-2 use is *dynamically* unreachable when RT is off.

`water.frag` has **no equivalent gate**: `traceWaterRay` / `foamShoreline` / the sun-shadow ray / the floor ray all run unconditionally in `main()` (water.frag declares `sceneFlags` but never reads `sceneFlags.x`). Meanwhile:
- (a) `WaterPipeline::new` is created unconditionally (not gated on `ray_query_supported`);
- (b) `reemit_water_planes` emits `WaterDrawCommand`s for every `WaterPlane` with no RT gate;
- (c) the water draw block is gated only on `!water_commands.is_empty()` && `self.water.is_some()`.

So on a Vulkan GPU lacking `VK_KHR_ray_query` / `acceleration_structure`, a water cell drives a draw whose shader statically uses set=1 binding=2 (TLAS) absent from the bound layout, with SPIR-V carrying the `RayQueryKHR` capability while the `rayQuery` device feature was left disabled.

**RT-hardware sub-case (REN-D15-NEW-01):** even with RT hardware, water.frag does not consult the per-frame TLAS-written bit, so on the first frame of a slot (before `write_tlas`) or a TLAS-failure frame it traces against an unwritten/stale TLAS — a readback cost the compute caustic path (`caustic_splat.comp`) explicitly avoids.

## Evidence
- `crates/renderer/src/vulkan/scene_buffer/buffers.rs` (`build_scene_descriptor_bindings`): `if rt_enabled { ...binding(2)...ACCELERATION_STRUCTURE_KHR... }` — binding 2 omitted when RT off; `let optional_bindings: &[u32] = if rt_enabled { &[] } else { &[2] };` with the comment that the shader still declares it because `rayQuery` calls are guarded by a uniform flag at runtime (holds for triangle.frag, **not** water.frag).
- `crates/renderer/src/vulkan/context/mod.rs:1763` — `WaterPipeline::new` is called regardless of `ray_query_supported` (only shader/pipeline *error* yields `None`); contrast `accel_manager = if device_caps.ray_query_supported { ... } else { None }`.
- `crates/renderer/shaders/water.frag:119` — `layout(set = 1, binding = 2) uniform accelerationStructureEXT topLevelAS;` used at lines 230/302/557/588 with **no** `sceneFlags.x` / `rtEnabled` guard. Contrast `crates/renderer/shaders/caustic_splat.comp:188` — `if (sceneFlags.x < 0.5) return;`.
- `byroredux/src/render/water.rs` (`reemit_water_planes`) — no `ray_query_supported` check.

## Impact
On non-RT hardware the engine has no explicit guard preventing the water draw.
- **Best case (most likely):** pipeline creation fails on the `RayQueryKHR`-capability-without-feature mismatch → `water = None`, water silently never renders (graceful but undocumented).
- **Worst case (driver-dependent, needs RenderDoc / a non-RT device to pin):** the pipeline creates and a ray query executes against a binding-2 slot absent from the bound layout → validation error / undefined behaviour / device loss.

Either way the "RT off ⇒ no ray queries run" contract that the whole `optional_bindings=[2]` design rests on is violated by the water path. NOT a concern on RT-capable hardware for binding presence (binding 2 is always a valid, possibly-empty TLAS there) — but the stale/unwritten-TLAS first-frame readback (REN-D15) still applies.

## Suggested Fix
Gate the water subsystem on RT support to match triangle.frag's contract:
- Cheapest: skip the water draw block in `draw.rs` when `!self.device_caps.ray_query_supported` (or `!self.scene_buffers.tlas_written[frame]`), and/or skip `WaterPipeline::new` on non-RT devices (mirroring `accel_manager` / `skin_compute` / `skin_palette`).
- Add `if (sceneFlags.x < 0.5)` early-outs around the water RT paths in `water.frag` (mirroring `caustic_splat.comp`) — also fixes the RT-hardware first-frame stale-TLAS sub-case (REN-D15-NEW-01).
- Belt-and-suspenders: add water.vert/water.frag to a `validate_set_layout` call with `optional_bindings=[2]` so layout/shader drift is caught at startup.

The pipeline-creation-failure-mode part is invisible to `cargo test` — verify on a non-RT device or with RenderDoc before assuming graceful degradation.

## Completeness Checks
- [ ] **SIBLING**: water.frag, caustic_splat.comp, and any other shader binding TLAS at set=1 binding=2 all share the `sceneFlags.x` gate
- [ ] **DROP**: `WaterPipeline` is created/destroyed in an order consistent with the conditional-creation change (mirror `accel_manager`)
- [ ] **STARTUP-VALIDATION**: water pipelines run through `validate_set_layout` with `optional_bindings=[2]`
- [ ] **RENDERDOC**: non-RT pipeline-creation failure vs ungated-ray-query path confirmed on a non-RT device / capture
- [ ] **TESTS**: a regression test pins the draw-loop / pipeline-creation RT gate
