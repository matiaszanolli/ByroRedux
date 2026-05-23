# #1197 — Investigation

## Buffer stability audit

For `SkinComputePipeline::dispatch` (3 bindings):
- **input_buffer** (`mesh_registry.global_vertex_buffer.buffer`):
  rebuilt on cell transition via `rebuild_geometry_ssbo` ([mesh.rs:641-675](../../crates/renderer/src/mesh.rs#L641-L675)).
  Handle changes infrequently (every cell load) but does change.
- **bone_buffer** (`scene_buffers.bone_buffers()[frame].buffer` — the
  palette SSBO): fixed handle for the renderer's lifetime ([scene_buffer/buffers.rs:501](../../crates/renderer/src/vulkan/scene_buffer/buffers.rs#L501)).
  Content updated each frame by `SkinPaletteComputePipeline::dispatch`,
  but the underlying `vk::Buffer` handle is stable.
- **output_buffer** (`slot.output_buffer.buffer`): fixed for the
  slot's lifetime ([skin_compute.rs:350-358](../../crates/renderer/src/vulkan/skin_compute.rs#L350-L358)).

For `SkinPaletteComputePipeline::dispatch` (3 bindings):
- **bone_world_buffer**: per-FIF DEVICE_LOCAL, fixed handle ([scene_buffer/upload.rs:698](../../crates/renderer/src/vulkan/scene_buffer/upload.rs#L698)).
- **bind_inverse_buffer**: persistent DEVICE_LOCAL, fixed handle.
- **palette_buffer**: per-FIF DEVICE_LOCAL, fixed handle.

For the palette pipeline ALL three are stable for the renderer
lifetime — descriptor writes are 100% redundant after first frame.

## Fix design

Self-healing **compare-and-skip** approach (cleaner than the issue's
"hook into MeshRegistry::rebuild_geometry_ssbo" plan because it doesn't
require knowing about cell transitions explicitly and is robust to any
future buffer-handle rotation):

1. Track per-FIF "last-bound buffer key" on the slot (or pipeline):
   - `SkinSlot.descriptor_bindings: [Option<(input, bone)>; MAX_FIF]`
     — output is implicit (a function of the slot itself).
   - `SkinPaletteComputePipeline.descriptor_bindings:
     [Option<(world, bind_inv, palette)>; MAX_FIF]`.
2. On dispatch, compare the current `(input, bone)` (or palette
   pipeline's triple) against the recorded key for `frame_index`.
   - Match → skip `update_descriptor_sets`. Bind + push + dispatch only.
   - Mismatch (or `None`) → emit the three writes, then record the
     new key.
3. Increment a per-frame counter on every actual write for the
   `tex.skin` instrumentation surface (#1194). Counter reset at frame
   start by `draw_frame`.

This produces:
- **First dispatch per FIF after slot creation**: 3 writes (cold).
- **Steady state**: 0 writes.
- **Cell transition** (input buffer rotates): 3 writes per slot for
  the FIF that re-enters first — auto-recovers as each FIF rotates
  through.

## Sibling check

- `SkinComputePipeline::dispatch` ([skin_compute.rs:437-499](../../crates/renderer/src/vulkan/skin_compute.rs#L437-L499))
  — main fix target.
- `SkinPaletteComputePipeline::dispatch` ([skin_compute.rs:705-766](../../crates/renderer/src/vulkan/skin_compute.rs#L705-L766))
  — has the same pattern; same fix.

## Counter surface

Add `descriptor_writes_this_frame: Cell<u32>` (interior mutability —
`Cell` is fine, dispatch is single-threaded). Expose via getters
`SkinComputePipeline::descriptor_writes_this_frame()` /
`reset_descriptor_writes_counter()`. The `tex.skin` debug surface and
the regression test compare before/after deltas.

## Scope

3 files: `skin_compute.rs`, `context/draw.rs` (mut access at call
sites + per-frame counter reset), and possibly the SkinSlot test
fixture for unit testing. Under 5-file threshold.

## Sync invariant (regression risk)

The per-frame fence at `draw_frame` top waits on `in_flight[frame]`
before this code runs ([context/draw.rs:188-209](../../crates/renderer/src/vulkan/context/draw.rs#L188-L209)),
so previous-frame use of `descriptor_sets[frame_index]` is complete
when we either rewrite OR skip. No new sync requirements. The
existing struct doc-comment about per-FIF fence ordering already
covers this case.
