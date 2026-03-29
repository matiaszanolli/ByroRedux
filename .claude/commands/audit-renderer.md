---
description: "Deep audit of the Vulkan renderer — pipeline, sync, memory, shaders"
---

# Renderer Audit

Read `_audit-common.md` and `_audit-severity.md` for shared protocol.

## Dimensions

### 1. Vulkan Synchronization
- Semaphore/fence lifecycle (signal before wait, no double-signal)
- images_in_flight tracking correctness
- Swapchain recreation: are all resources properly waited on and destroyed?
- Queue submit ordering (graphics vs present)

### 2. GPU Memory
- gpu-allocator usage: correct memory types (CpuToGpu vs GpuOnly)
- Buffer/image destruction before allocator drop
- Allocator dropped before device destruction
- No leaked VkDeviceMemory on shutdown

### 3. Pipeline State
- Vertex input matches shader layout (binding, location, format, offset)
- Push constant ranges match shader declarations
- Dynamic state correctly set each frame (viewport, scissor)
- Pipeline compatible with render pass (attachment formats, subpass)

### 4. Render Pass
- Attachment load/store ops (CLEAR + STORE for color)
- Layout transitions (UNDEFINED → COLOR_ATTACHMENT → PRESENT_SRC)
- Subpass dependencies cover all stage/access masks

### 5. Command Buffer Recording
- Reset before re-record (RESET_COMMAND_BUFFER flag on pool)
- Begin/end balanced
- Render pass begin/end balanced
- No commands recorded outside render pass that require it (or vice versa)

### 6. Shader Correctness
- SPIR-V matches GLSL source (recompile and diff)
- Push constant struct layout matches Rust-side byte offsets
- Vertex attribute locations match Vertex struct field order

### 7. Resource Lifecycle
- All VkShaderModule, VkPipeline, VkPipelineLayout destroyed in Drop
- Framebuffers destroyed before render pass
- Swapchain image views destroyed before swapchain
- Device destroyed after all resources

## Process

1. Read each file in `crates/renderer/src/vulkan/` and `crates/renderer/src/`
2. Check each dimension against the code
3. Record findings per the shared finding format
4. Save report to `docs/audits/AUDIT_RENDERER_<TODAY>.md`
