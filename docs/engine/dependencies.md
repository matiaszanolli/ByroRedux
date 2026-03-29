# Dependencies

All external dependencies are declared in the workspace root `Cargo.toml`
and consumed by individual crates via `{ workspace = true }`.

## External Dependencies

| Crate | Version | Used By | Purpose |
|-------|---------|---------|---------|
| ash | 0.38 | renderer | Raw Vulkan bindings |
| ash-window | 0.13 | renderer | Surface creation from window handles |
| gpu-allocator | 0.27 | renderer | GPU memory allocation |
| winit | 0.30 | platform, renderer, binary | Cross-platform windowing |
| raw-window-handle | 0.6 | platform, renderer | Platform-agnostic window handle traits |
| glam | 0.29 | core | Linear algebra (Vec2/3/4, Mat3/4, Quat) |
| log | 0.4 | all crates | Logging facade |
| env_logger | 0.11 | binary | Log output to stderr |
| anyhow | 1 | renderer, platform, binary | Error handling with context |
| thiserror | 2 | core, renderer | Derive macro for error types |
| string-interner | 0.17 | core | String interning for FixedString |
| cxx | 1 | cxx-bridge | C++ interop FFI |
| cxx-build | 1 | cxx-bridge (build) | C++ compilation for cxx bridge |

## Internal Crate Dependencies

| Crate | Depends On |
|-------|-----------|
| byroredux-core | (none — leaf crate) |
| byroredux-platform | core |
| byroredux-renderer | core, platform |
| byroredux-scripting | core |
| byroredux-cxx-bridge | (none) |
| byroredux (binary) | all of the above |

## Dependency Philosophy

- **Minimal.** No framework dependencies. Individual focused crates.
- **Workspace-managed.** All versions in one place.
- **No duplicates.** Each capability covered by exactly one crate.
- **gpu-allocator** is included but not yet used — it's needed for
  the next milestone (actual geometry rendering with vertex buffers).
