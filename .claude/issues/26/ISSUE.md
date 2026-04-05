# Issue #26: Renderer: TLAS rebuild calls device_wait_idle mid-frame

- **State**: OPEN
- **Labels**: bug, renderer, medium, sync
- **Location**: `crates/renderer/src/vulkan/acceleration.rs:247`

device_wait_idle() stalls entire GPU when TLAS instance count exceeds
capacity during command buffer recording. 5-20ms hitch on scene load.
