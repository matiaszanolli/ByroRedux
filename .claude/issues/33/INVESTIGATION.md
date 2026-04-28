# Issue #33 — Investigation

## Path correction
Stale path in audit. `context.rs` was split; teardown now lives in:
- `crates/renderer/src/vulkan/context/mod.rs:1389-1559` — `Drop`
- `crates/renderer/src/vulkan/context/resize.rs:17-449` — `recreate_swapchain`
- `crates/renderer/src/vulkan/context/helpers.rs` — stateless helpers

## Common destruction sequences

Both call sites destroy three resource categories in identical relative order:

1. **Framebuffers** — `destroy_framebuffer(fb)` per fb
2. **Depth resources** — view → image → free allocation (Vulkan VUID-vkFreeMemory-memory-00677)
3. **Render-pass-bound pipelines** — `pipeline`, `pipeline_two_sided`, `blend_pipeline_cache` drain, `pipeline_ui`

## Why a shared helper, not a single combined helper

Drop interleaves subsystem destruction (texture_registry, gbuffer, composite, svgf, taa…) **between** framebuffers and depth — those subsystems own the image views the framebuffers and pipelines reference, so their teardown order matters. Resize does not destroy subsystems (they expose `recreate_on_resize`).

Therefore the helpers must be **separately callable**, not merged into one teardown function. Three small focused helpers in `helpers.rs` are the right shape.

## Plan

Add to `helpers.rs`:
- `destroy_main_framebuffers(&device, &mut framebuffers)`
- `destroy_depth_resources(&device, &allocator, &mut view, &mut image, &mut allocation)`
- `destroy_render_pass_pipelines(&device, &mut p, &mut p2, &mut blend_cache, &mut p_ui)`

Each nulls handles after destruction (idempotent — safe for resize's "destroy then maybe rebuild" path; no-op for Drop). Replace the duplicated loops at both sites.

Files touched: 3 (helpers.rs, mod.rs, resize.rs). Within scope.
