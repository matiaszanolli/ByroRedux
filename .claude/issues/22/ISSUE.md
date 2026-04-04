# Issue #22: Renderer: TLAS instance buffer missing HOST→AS_BUILD barrier

- **State**: OPEN
- **Labels**: bug, renderer, high, sync
- **Location**: `crates/renderer/src/vulkan/acceleration.rs:337-372`

Instance buffer written via write_mapped() then consumed by
cmd_build_acceleration_structures with no pipeline barrier.
Vulkan spec requires HOST→AS_BUILD dependency.
