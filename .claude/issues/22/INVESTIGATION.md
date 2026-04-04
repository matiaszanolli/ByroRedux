# Investigation: Issue #22

## Root Cause
In `build_tlas()`:
- Line 339: `tlas.instance_buffer.write_mapped(device, &instances)` — host write
- Line 374: `cmd_build_acceleration_structures(cmd, ...)` — GPU reads instance buffer

No memory barrier between these operations. The Vulkan spec requires a
HOST→ACCELERATION_STRUCTURE_BUILD dependency to ensure the host write is
visible to the AS build input read.

## BLAS Sibling Check
BLAS build (line 164) uses `with_one_time_commands` — vertex/index data
was uploaded via staging with a prior completed submit. The implicit
queue submit ordering provides the needed sync. No barrier needed.

## Fix
Insert `cmd_pipeline_barrier` between write_mapped and the build:
- srcStageMask: HOST
- dstStageMask: ACCELERATION_STRUCTURE_BUILD_KHR
- srcAccessMask: HOST_WRITE
- dstAccessMask: ACCELERATION_STRUCTURE_READ_KHR (input read)

Use a buffer memory barrier targeting the instance buffer specifically.

## Scope
1 file: acceleration.rs. ~10 lines inserted.
