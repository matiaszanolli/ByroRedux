# SAFE-2026-10-27-01: `SceneBuffers::destroy` never frees the #6250 `moved_surface_buffers`, so every shutdown leaks the device (release) or panics (debug)

Labels: high, safety, renderer, vulkan, memory, bug
Source: docs/audits/AUDIT_SAFETY_2026-10-27.md

**Audit report**: `docs/audits/AUDIT_SAFETY_2026-10-27.md` -- finding `SAFE-2026-10-27-01` (HIGH)
**Location (re-mapped to current code)**: `crates/renderer/src/vulkan/scene_buffer/buffers.rs` (field :237, creation :663, seeded :1024); `crates/renderer/src/vulkan/scene_buffer/descriptors.rs` (`destroy` ~:361-426, field absent; `selected_ray_probe_buffers` arm at ~:412); `crates/renderer/src/vulkan/context/teardown.rs` (~:454-497, failing `Arc::try_unwrap`). Premise re-read at HEAD: `grep moved_surface_buffers` shows no destroy/clear site anywhere in `crates/renderer/src`.
**Status at filing**: NEW (regression introduced by bed162262, Fix #6250, closed). Also reported as REN-D5-01 in AUDIT_RENDERER_2026-10-27 and as a dup in AUDIT_REGRESSION_2026-10-27 -- publish once.

## Description
`SceneBuffers::destroy` destroys and `.clear()`s every other `Vec<GpuBuffer>` (light, camera, bone palette, instance, previous-model, material, DALC, indirect, selected-ray-probe). #6250 added `moved_surface_buffers` (one host-visible `GpuBuffer` per frame in flight), seeded it at construction and rewrote it each frame (`upload.rs` ~:841-863), but never added it to `destroy`. Each `GpuBuffer` holds a `SharedAllocator` (`Arc<Mutex<Allocator>>`) clone, so at `VulkanContext::drop` the allocator strong count is 3 (the context's own Arc + the two frames-in-flight buffers). `Arc::try_unwrap` fails and teardown takes the #665 leak-everything branch.

## Evidence
CI run 40596734340 (eae8fd740), job `Vulkan validation layers (lavapipe)`:
```
ERROR byroredux_renderer::vulkan::context::teardown] GPU allocator has 3 outstanding references -- leaking allocator + device + surface + instance ...
thread 'main' panicked at crates/renderer/src/vulkan/context/teardown.rs:496:25:
GPU allocator leaked: outstanding Arc references
bench exit status: 101
```
The same log shows `rt-integrity ... verdict=PASS` and no `[Vulkan]` validation lines, so the panic is the lane's only failure. The lane was last green at 79c8322f4, first red at c7463c04d (first CI run including bed162262), and has been red on every run since (28 runs). A field-by-field read confirms this is the only GPU-resource field added since the baseline with no destroy.

## Impact
- Release: teardown skips `vkDestroyDevice`, surface, instance and debug messenger; the OS reclaims them at exit, so the "must terminate to reclaim" path becomes the normal shutdown.
- Debug: `debug_assert!(false)` panics inside `Drop`; every debug bench exit returns 101.
- CI: the lavapipe validation lane is blind -- a real validation regression landing now would sit behind an already-red lane.
- Not a per-frame leak and no use-after-free (the Drop safety net runs against the deliberately leaked, still-live device).

## Related
#665, #927, #4187, #732 (closed, same symptom on other fields); #6250 (closed, origin); REN-D5-01 (renderer report, same root cause).

## Suggested Fix
1. Add `for buf in &mut self.moved_surface_buffers { buf.destroy(device, allocator); } self.moved_surface_buffers.clear();` beside the `selected_ray_probe_buffers` arm in `SceneBuffers::destroy`.
2. Add a source-scan guard that every `Vec<GpuBuffer>` / `GpuBuffer` field declared on `SceneBuffers` appears in `destroy` (nothing pins this today; the lavapipe lane is the only detector).

## Completeness Checks
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix

