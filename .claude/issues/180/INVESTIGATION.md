# Investigation: #180 Push constants exceed 128-byte spec minimum

## Root Cause
Push constants: viewProj (mat4, 64B) + model (mat4, 64B) + boneOffset (uint, 4B) = 132 bytes.
Vulkan guarantees only `maxPushConstantsSize >= 128`. Comment in pipeline.rs incorrectly says "well under."

## Fix
Move `viewProj` from push constants to the camera UBO (set 1, binding 1). It's per-frame, not
per-draw, so it belongs in the UBO anyway (reduces redundant per-draw push constant traffic).

- Push constants: model (64B) + boneOffset (4B) = 68 bytes (well under 128)
- Camera UBO: viewProj (64B) + cameraPos (16B) + sceneFlags (16B) = 96 bytes

## Files Changed
- `crates/renderer/src/vulkan/scene_buffer.rs` — GpuCamera gets view_proj field
- `crates/renderer/src/vulkan/pipeline.rs` — push constant size 132→68
- `crates/renderer/src/vulkan/context/draw.rs` — adjust push offsets, pass viewProj to upload_camera
- `crates/renderer/shaders/triangle.vert` — read viewProj from CameraUBO
- `crates/renderer/shaders/triangle.frag` — add viewProj to CameraUBO (must match)
- Recompile SPIR-V
