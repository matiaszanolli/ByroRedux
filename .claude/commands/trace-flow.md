---
description: "Trace a data flow through the engine — from ECS to GPU or from file to entity"
argument-hint: "<flow-name or description>"
---

# Flow Tracer

Trace how data moves through the engine for a named flow.

## Built-in Flows

### `render`
Entity with (Transform, MeshHandle) → build_render_data() → DrawCommand → VulkanContext::draw_frame() → push constants → cmd_draw_indexed

### `frame-tick`
winit about_to_wait → Instant::now() → DeltaTime/TotalTime update → scheduler.run() → systems execute → request_redraw → RedrawRequested → draw_frame

### `entity-spawn`
World::spawn() → EntityId → world.insert(id, Component) → storage_write() → RwLock → TypeMap entry → storage.insert()

### `query`
world.query::<T>() → storages.get(TypeId) → RwLock::read() → QueryRead holding guard → iter() → (EntityId, &T) pairs

### `resize`
WindowEvent::Resized → recreate_swapchain() → destroy old (framebuffers, render pass, swapchain) → create new → reset_image_fences → update Camera aspect

### `nif-import` (planned)
.nif file → NIF parser (header, objects, RTTI, strings) → link resolution → NIF-to-ECS importer → spawn entities with (Transform, MeshHandle, Name, Parent, Children)

## Custom Flows

If `$ARGUMENTS` doesn't match a built-in flow, trace the described data path:
1. Find the entry point (grep for the function/type)
2. Follow each call step by step, noting file:line
3. Document data transformations at each boundary
4. Identify risks: error handling gaps, missing validation, performance concerns

## Output Format

```
Step 1: [file:line] description
  → data: what's passed
Step 2: [file:line] description
  → transforms: what changes
...
```
