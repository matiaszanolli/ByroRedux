# 3244: D2-01: MorphSlot weight buffer raced by host write with no per-frame-in-flight double buffering

**Severity**: HIGH · **Report**: `docs/audits/AUDIT_RENDERER_2026-08-24.md` (D2-01) · also independently found as `CONC-D2-2026-08-24-01` in `docs/audits/AUDIT_CONCURRENCY_2026-08-24.md`

## Description

Every other per-frame-mutated GPU-read resource in this codebase (`camera_buffers`, `light_buffers`, `bone_world_staging_buffers`, `instance_buffers`, `dalc_buffers`) is double-buffered by `[frame_index]`. `MorphSlot` allocates **one** `weight_buffer` at spawn time and never again — not `[frame_index]`-indexed, its GPU address cached once as a `buffer_reference` handle. `render::skinned::update_morph_weights` overwrites that single buffer's host-visible memory unconditionally every frame, called from `app_frame.rs:169` — *before* `ctx.draw_frame()` is reached later in the same `render_one_frame` call. `draw_frame`'s `wait_for_fences([in_flight[frame], in_flight[prev]], ...)` — the only synchronization point that could prove the prior frame's GPU work has finished touching this buffer — runs *inside* `draw_frame`, strictly after the host write for the current iteration has already landed.

## Location
- `crates/renderer/src/vulkan/morph_compute.rs` (`MorphSlot::weight_buffer`, `MorphSlot::update_weights`)
- `byroredux/src/render/skinned.rs` (`update_morph_weights`)
- `byroredux/src/app_frame.rs:169`
- `crates/renderer/src/vulkan/context/draw.rs:1604-1626`

## Evidence

```rust
// byroredux/src/app_frame.rs:165-169 — write happens before draw_frame()
crate::render::update_morph_weights(&self.world, ctx);
// draw_frame() called later in the same function (line 474)
```
```rust
// morph_compute.rs — ONE buffer, created once at spawn, never re-created:
pub struct MorphSlot {
    weight_buffer: GpuBuffer,   // no [frame_index] array
    weight_address: vk::DeviceAddress, // cached once
}
```

## Impact

Any entity with a live `AnimatedMorphWeights` component (facial expressions, blinking, talking) writes its weight buffer every frame while morph animation is active. If the GPU is running behind the CPU by even a fraction of a frame (normal under MAILBOX present mode, which this engine selects), the shader reads a torn mix of this-frame and previous-frame weights via both `triangle.vert` (primary raster) and `skin_vertices.comp` (feeds skinned-BLAS geometry hit by RT shadow/reflection/GI rays). Failure mode is a transient, self-correcting per-vertex morph glitch, not a crash — but it is a genuine unsynchronized host/device access with no execution dependency between the write and the prior read, meeting the "Vulkan spec violation = at least HIGH" severity floor.

This bug was found independently by two audits (Renderer D2-01, Concurrency CONC-D2-2026-08-24-01) — filed once here.

## Suggested Fix

Give `MorphSlot` two weight buffers mirroring `bone_world_staging_buffers[frame_index]`/`bone_world_device_buffers[frame_index]`, publishing the address for the current `frame_index` one frame ahead of the read that consumes it. Alternatively, move `update_morph_weights` to run *after* `draw_frame`'s dual-fence wait (one frame of morph-weight input latency) — closes the hole without a second buffer.

## Completeness Checks
- [ ] **SIBLING**: Same double-buffering pattern checked against other single-shot GPU-address-cached resources
- [ ] **TESTS**: A regression test pins this specific fix
