# Investigation: #243 (PERF-04-11-M7) — per-frame Vec allocations in draw_frame

## Domain
renderer (CPU hot path)

## Current state
At `crates/renderer/src/vulkan/context/draw.rs:279-280`:
```rust
let mut gpu_instances: Vec<GpuInstance> = Vec::with_capacity(draw_commands.len());
let mut batches: Vec<DrawBatch> = Vec::new();
```
Two heap allocations per frame in the hot path. Capacity is lost between frames — each frame starts from zero and grows. At 60 FPS that is 120 allocs/sec.

## Fix
Move both vectors to fields on `VulkanContext`, mirroring the `make_transform_propagation_system` closure-captured-scratch pattern already used in the ECS layer.

`draw_frame` uses `std::mem::take` to move the scratch buffers out of `&mut self`, `clear()` them, `reserve(draw_commands.len())` them, runs the batch-build loop, and at the bottom of the function puts them back. This leaves the rest of `draw_frame` free to call methods on other fields of `self` without the borrow checker tripping on a long-lived mutable borrow of one field. Error-path early returns lose the amortization for one frame only — acceptable since the draw has already failed.

`DrawBatch` needs to be nameable from `context/mod.rs` for the struct field type; it was `struct DrawBatch` (private to draw.rs) and is now `pub(super) struct DrawBatch` with pub fields. The scratch field itself stays module-private (no visibility modifier) so the type visibility matches.

## Sibling check
Per the issue's completeness checks, `build_render_data`'s `draw_commands`/`gpu_lights` is a known-related pattern — already tracked as PERF-18 (dim 6 finding 2) from a prior audit. Not touched here to keep scope minimal; it's the next logical fix in the same family.

## Files touched
- `crates/renderer/src/vulkan/context/mod.rs` — 2 new scratch fields on `VulkanContext`, initialized in `new()`
- `crates/renderer/src/vulkan/context/draw.rs` — `DrawBatch` visibility lifted; `draw_frame` uses mem::take + restore pattern

2 files, under scope ceiling.
