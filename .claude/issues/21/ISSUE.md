# Issue #21: Renderer: SSBO/UBO mapped writes assume HOST_COHERENT without verification

- **State**: OPEN  
- **Labels**: bug, renderer, high, vulkan
- **Location**: `buffer.rs:154-167`, `context.rs:464-484`

write_mapped() copies to mapped memory with no flush. CpuToGpu prefers
HOST_COHERENT but doesn't guarantee it.

**Fix**: Check memory_properties() on allocation. If not HOST_COHERENT,
flush after write. Or verify at allocation and bail/warn.
