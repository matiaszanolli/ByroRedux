# Investigation: #179 TLAS shared across frames-in-flight

## Root Cause
`AccelerationManager` has a single `TlasState` (accel, result buffer, instance buffer) and a single
`scratch_buffer`. With `MAX_FRAMES_IN_FLIGHT=2`, both frame slots record TLAS builds into overlapping
command buffers that reference the same GPU resources.

The fence wait at `draw.rs:36` only guarantees the *current frame slot's* previous work is done —
it does NOT wait for the other frame slot. So frame slot 0's TLAS build can overlap with frame slot 1's
TLAS build on the GPU.

## Hazards
1. **Instance buffer**: host-written per frame → write-after-read if other frame's GPU build reads it
2. **Scratch buffer**: GPU-written during build → write-after-write between overlapping builds
3. **TLAS accel/result buffer**: GPU-written during build, read during fragment shader ray queries → WAR

## Fix
Per-frame-in-flight TLAS state. Each slot gets its own:
- `TlasState` (accel, buffer, instance_buffer)
- scratch buffer

The fence wait guarantees each slot's previous use is complete, so no additional sync needed.
The `device_wait_idle` in the resize path becomes unnecessary — only the current slot's old TLAS
is destroyed, and it was just fence-waited.

## Files Changed
- `crates/renderer/src/vulkan/acceleration.rs` — double-buffer tlas + scratch
- `crates/renderer/src/vulkan/context/draw.rs` — pass frame_index to build_tlas/tlas_handle
