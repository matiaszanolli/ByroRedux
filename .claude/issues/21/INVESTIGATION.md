# Investigation: Issue #21

## Root Cause
`write_mapped()` in buffer.rs copies data to mapped memory via
`alloc.mapped_slice_mut()` but never calls `vkFlushMappedMemoryRanges`.
gpu-allocator's CpuToGpu prefers HOST_COHERENT but the Vulkan spec
doesn't mandate it. On a non-coherent allocation, GPU would read stale data.

## Callers of write_mapped
- context.rs upload_lights() — writes light SSBO per frame  
- context.rs upload_camera() — writes camera UBO per frame
- acceleration.rs — writes instance buffer per frame
- staging buffer in create_device_local_buffer (but that uses one-time
  commands with implicit host→device sync via submit)

## Fix Options
1. **Assert coherent at alloc** — simplest, but panics on exotic hardware
2. **Flush in write_mapped** — needs device handle
3. **Store coherent flag, provide flush_if_needed()** — cleanest

Going with option 3: store a `is_coherent` flag on GpuBuffer, add
`flush_if_needed(device)` that callers invoke after write_mapped.

Actually, even simpler: `write_mapped` already requires `&mut self`, so
I can store the device-relevant info. But the cleanest approach for the
callers: add `device` parameter to `write_mapped` and flush internally
if needed. This is a one-shot fix — all callers already have the device.

## Scope
1 file (buffer.rs): add coherent tracking + conditional flush in write_mapped.
All callers need to pass `device` — checking call sites.
