# Issue #25: Renderer: AS buffers use HOST_VISIBLE instead of DEVICE_LOCAL memory

- **State**: OPEN
- **Labels**: bug, renderer, medium, memory
- **Location**: `crates/renderer/src/vulkan/acceleration.rs:114-140, :297-324`

BLAS/TLAS result buffers and scratch buffers use `create_host_visible` (CpuToGpu).
These are purely GPU-populated and GPU-read. On discrete GPUs, AS traversal goes over PCIe.

**Fix**: Add `GpuBuffer::create_device_local` for result + scratch buffers.
Keep `create_host_visible` only for instance buffer (CPU-written per frame).
