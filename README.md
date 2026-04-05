# ByroRedux

A clean rebuild of the Gamebryo/Creation engine lineage in **Rust + C++**, using **Vulkan** for rendering. Linux-first, with the long-term goal of loading and running content from Gamebryo/Creation-era games (Oblivion, Fallout 3, New Vegas, Skyrim).

Not a port — a ground-up rebuild that understands the legacy architecture and builds modern equivalents.

![*Prospector Saloon* from Fallout: New Vegas, rendered in ByroRedux](docs/screenshots/prospector-saloon.png)

*The Prospector Saloon (Goodsprings) loaded from FalloutNV.esm — 789 entities, RT multi-light with shadows, cell interior XCLL lighting at 85 FPS on RTX 4070 Ti.*

## Current State

**22 milestones complete (M1–M22).** Loads full Fallout: New Vegas interior/exterior cells and Skyrim SE meshes with RT ray-traced shadows. Multi-light rendering with cell ambient/directional from ESM. Animation playback with scene graph hierarchy. NIF parser overhaul in progress (N23 series) — skinning, Havok collision, blend interpolators, vertex color/stencil/zbuffer properties, and Material ECS component now landed.

```bash
# FNV interior cell
cargo run -- --esm FalloutNV.esm --cell GSProspectorSaloonInterior \
             --bsa "Fallout - Meshes.bsa" \
             --textures-bsa "Fallout - Textures.bsa" \
             --textures-bsa "Fallout - Textures2.bsa"

# Skyrim SE mesh with textures
cargo run -- --bsa "Skyrim - Meshes0.bsa" \
             --mesh "meshes\clutter\ingredients\sweetroll01.nif" \
             --textures-bsa "Skyrim - Textures3.bsa"

# Animation playback
cargo run -- path/to/mesh.nif --kf path/to/anim.kf

# SWF menu overlay
cargo run -- --swf path/to/menu.swf
```

Press **Escape** to capture mouse, then **WASD** + mouse to fly around. **Space/Shift** for up/down, **Ctrl** for speed boost.

| Feature | Status |
|---------|--------|
| ECS with pluggable storage (SparseSet + Packed), hierarchy (Parent/Children) | Working |
| Vulkan RT renderer with multi-light SSBO, ray query shadows, cell XCLL lighting | Working |
| ESM parser (CELL, REFR, STAT, MSTT, FURN, DOOR, ACTI, CONT, LIGH, XCLL + 23 types) | Working |
| NIF parser (107 block types, shader trailing fields, skinning, multi-bound, Havok skip) | Working |
| DDS texture loading (BC1/BC3/BC5 + DX10, mipmaps, shared sampler cache) | Working |
| BSA v103 + v104 + v105 archive reader (Oblivion/FO3/FNV/Skyrim LE/SE) | Working |
| Interior + exterior cell loading with placed object transforms | Working |
| Animation playback (.kf files, linear/Hermite/TBC, 8 controller types, blending stack) | Working |
| NiControllerManager embedded animation discovery | Working |
| Text key events as transient ECS markers | Working |
| Scene graph hierarchy (Parent/Children) with transform propagation | Working |
| Skyrim SE NIF support (BSTriShape, BSLightingShader, packed vertices) | Working |
| Scaleform/SWF UI system (Ruffle integration, deferred texture updates) | Working |
| Pipeline cache with disk persistence | Working |
| Fly camera (WASD + mouse look) | Working |
| Plugin system with stable Form IDs, conflict resolution | Working |
| ECS-native scripting (events, timers) | Working |
| Material component (emissive, specular, glossiness, UV, normal map) | Working |
| WorldBound component (bounding sphere for frustum culling / spatial queries) | Working |
| StagingPool for reusable GPU upload buffers | Working |
| 312 unit tests | Passing |

## Architecture

```
byroredux/                 Binary — game loop, cell loader, fly camera, animation system
crates/
  core/                    ECS, math (glam), animation engine, types, string interning, form IDs
  renderer/                Vulkan graphics via ash + gpu-allocator
  plugin/                  Plugin system, ESM parser, manifests, conflict resolution
  nif/                     NIF file parser + animation importer (.nif/.kf)
  bsa/                     BSA archive reader (v103/v104/v105)
  ui/                      Scaleform/SWF UI system (Ruffle integration)
  scripting/               ECS-native scripting (events, timers)
  platform/                Windowing via winit (Linux-first)
  cxx-bridge/              C++ interop via cxx
```

### ECS Design

Components declare their own storage backend — no runtime dispatch:

- **SparseSetStorage** — O(1) insert/remove via swap-remove. Default for gameplay data.
- **PackedStorage** — Sorted by entity ID, cache-friendly iteration. Opt-in for hot-path components.

World wraps each storage in `RwLock` so query methods take `&self`, enabling concurrent reads across different component types. Multi-component queries acquire locks in `TypeId` order to prevent deadlocks. Zero unsafe blocks in the ECS module — `ComponentRef` guard pattern ensures sound lifetime management.

### Renderer

Vulkan 1.3 via `ash` with RT ray query extensions:

- Full initialization chain with validation layers in debug builds
- RT acceleration structures (BLAS per mesh + TLAS per frame) with DEVICE_LOCAL memory
- Multi-light SSBO with point/spot/directional lights and RT shadow rays
- Pipeline cache with disk persistence (10-50ms cold → <1ms warm)
- Shared VkSampler across all textures
- Per-image semaphore synchronization with HOST→AS_BUILD memory barriers
- Deferred texture destruction for stall-free dynamic UI updates
- Atomic swapchain handoff on resize
- Proper depth format querying with fallback chain (D32→D32S8→D24S8→D16)
- Backface culling with confirmed NIF/D3D CW winding convention

### Asset Pipeline

ESM files parsed for CELL/REFR/STAT records (23 record types). Interior cell lighting from XCLL subrecords. NIF files parsed with version-aware binary reading (NifVariant for 8 game variants, 107 registered block types including 30 Havok collision blocks). Shader-type trailing fields fully parsed (env map, skin/hair tint, parallax, eye cubemap). Skinning blocks fully parsed (NiSkinInstance/Data/Partition, BsDismemberSkinInstance). Multi-bound spatial volumes (BSMultiBound/AABB/OBB). Scene graph hierarchy preserved as Parent/Children entities. Single-pass material property extraction into Material ECS component (emissive, specular, glossiness, UV, normal map, env map scale). DDS textures from BSA v103/v104/v105 archives with BC-compressed mipmap chains. StagingPool for reusable GPU upload buffers. NiControllerManager embedded animation discovery with text key event emission.

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
cargo run                          # Demo scene
cargo run -- path/to/mesh.nif      # Render a loose NIF file
cargo run -- mesh.nif --kf anim.kf # Play animation on a mesh
cargo run -- --esm FalloutNV.esm \
             --cell CellEditorID \
             --bsa "Fallout - Meshes.bsa" \
             --textures-bsa "Fallout - Textures.bsa"  # Load an interior cell
cargo run -- --swf menu.swf        # Render a Scaleform SWF menu
cargo run -- --debug               # Show FPS/entity stats in title bar
cargo test                         # Run all 312 tests
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
- [Development Roadmap](ROADMAP.md) — milestones, game compatibility, known issues

## Stats

| Metric | Value |
|--------|-------|
| Rust source files | 100 |
| Lines of Rust | ~26,800 |
| Unit tests | 312 |
| Commits | 161 |
| Workspace crates | 10 |

## Dependencies

| Crate | Purpose |
|-------|---------|
| ash | Raw Vulkan bindings |
| gpu-allocator | GPU memory allocation |
| winit | Cross-platform windowing |
| glam | Linear algebra |
| nalgebra | SVD for degenerate rotation repair |
| string-interner | O(1) string equality |
| cxx | C++ interop |
| image | PNG/image loading |

## License

MIT
