# ByroRedux

A clean rebuild of the Gamebryo/Creation engine lineage in **Rust + C++**, using **Vulkan** for rendering. Linux-first, with the long-term goal of loading and running content from Gamebryo/Creation-era games (Oblivion, Fallout 3, New Vegas, Skyrim).

Not a port — a ground-up rebuild that understands the legacy architecture and builds modern equivalents.

## Architecture

```
byroredux/            Binary — game loop entry point
crates/
  core/                    ECS, math (glam), types, string interning
  renderer/                Vulkan graphics via ash + gpu-allocator
  platform/                Windowing via winit (Linux-first)
  scripting/               Placeholder for embedded scripting
  cxx-bridge/              C++ interop via cxx
```

### ECS Design

Components declare their own storage backend — no runtime dispatch:

- **SparseSetStorage** — O(1) insert/remove via swap-remove. Default for gameplay data.
- **PackedStorage** — Sorted by entity ID, cache-friendly iteration. Opt-in for hot-path components.

World wraps each storage in `RwLock` so query methods take `&self`, enabling concurrent reads across different component types. Multi-component queries acquire locks in `TypeId` order to prevent deadlocks.

### Renderer

Full Vulkan initialization chain (13 steps) via `ash`. No shortcuts:

Instance + validation layers, debug messenger, surface, physical/logical device, swapchain, render pass, graphics pipeline with push constants, framebuffers, command pool/buffers, GPU memory allocation via `gpu-allocator`, per-image synchronization.

## Current State

- ECS with pluggable storage, system scheduler, resources, string interning
- Vulkan renderer: graphics pipeline, vertex/index buffers, push constants (MVP matrices)
- ECS-driven spinning cube with perspective camera
- C++ interop bridge operational
- 68 unit tests

## Building

### Prerequisites

- Rust (stable, 2021 edition)
- Vulkan SDK or drivers with validation layers
- `glslangValidator` for shader compilation
- C++17 compiler (for cxx bridge)
- Linux (primary target)

### Build & Run

```bash
cargo build
cargo run        # Opens window with spinning cube
cargo test       # Run all tests
```

### Shader Compilation

Shaders are pre-compiled to SPIR-V and included via `include_bytes!`:

```bash
cd crates/renderer/shaders
glslangValidator -V triangle.vert -o triangle.vert.spv
glslangValidator -V triangle.frag -o triangle.frag.spv
```

## Documentation

- [Engine Documentation](docs/engine/index.md) — architecture, ECS, renderer, game loop
- [Legacy Gamebryo 2.3 Analysis](docs/legacy/gamebryo-2.3-architecture.md) — class hierarchy, NIF format, compatibility mapping
- [Development Roadmap](docs/engine/architecture.md)

## Roadmap

| Phase | Status | Milestone |
|-------|--------|-----------|
| 1. Hardcoded triangle | Done | Graphics pipeline end-to-end |
| 2. GPU vertex buffers | Done | Geometry from Rust data via gpu-allocator |
| 3. ECS-driven rendering | Done | Spinning cube, perspective camera, push constants |
| 4. Plugin system (core) | Done | Stable Form IDs, FormIdPool, FormIdComponent |
| 5. Plugin system (data) | Done | Plugin manifests, DataStore, DAG-based conflict resolution |
| 6. Legacy bridge | Done | ESM/ESP/ESL/ESH Form ID conversion, per-game parser stubs |
| 7. Depth buffer | Done | Correct occlusion, multiple objects |
| 8. Texturing | Done | Staging upload, descriptor sets, sampled checkerboard |
| 9. NIF parser | Next | Parse Gamebryo .nif files |
| 10. NIF-to-ECS import | Planned | Load and render legacy meshes |
| 11. Animation | Planned | Keyframe playback from .kf files |

## Dependencies

| Crate | Purpose |
|-------|---------|
| ash | Raw Vulkan bindings |
| gpu-allocator | GPU memory allocation |
| winit | Cross-platform windowing |
| glam | Linear algebra |
| string-interner | O(1) string equality |
| uuid | Stable plugin identity (UUID v5) |
| semver | Plugin version parsing |
| serde + toml | Plugin manifest parsing |
| image | PNG/texture loading |
| cxx | C++ interop |

## License

MIT
