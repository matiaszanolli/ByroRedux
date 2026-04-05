# Investigation: Issue #26

## Root Cause
acceleration.rs:247: `device.device_wait_idle().ok()` inside build_tlas()
during command buffer recording. Triggered when instance_count exceeds
current TLAS max_instances, requiring reallocation.

## Why device_wait_idle Exists
Old TLAS may be referenced by in-flight command buffers. Need to ensure
it's not in use before destroying. But device_wait_idle is the nuclear
option — stalls entire GPU pipeline.

## Fix Strategy
Two-pronged:
1. **Pre-size generously**: Start with 4096 max_instances instead of 64.
   Interior cells are ~200-800 objects, exterior ~1000-3000. 4096 covers
   most cases without resize.
2. **Higher growth factor**: When resize IS needed, grow to 2x instead of
   next_power_of_two (which might be small for near-power values).
3. **Keep device_wait_idle as safety net** but log a warning so we know
   it triggered. In practice it should never fire with 4096 initial.

A proper deferred cleanup queue (frame-latency destruction) is the ideal
solution but is significantly more complex — defer to a follow-up.

## Scope
1 file: acceleration.rs. Change initial capacity + growth + add warning.
