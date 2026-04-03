# ByroRedux

A clean rebuild of the Gamebryo/Creation engine lineage in **Rust + C++**, using **Vulkan** for rendering. Linux-first, with the long-term goal of loading and running content from Gamebryo/Creation-era games (Oblivion, Fallout 3, New Vegas, Skyrim).

Not a port — a ground-up rebuild that understands the legacy architecture and builds modern equivalents.

![Prospector Saloon from Fallout: New Vegas, rendered in ByroRedux](docs/screenshots/prospector-saloon.png)

*The Prospector Saloon (Goodsprings) loaded from FalloutNV.esm — 809 entities, real DDS textures, directional lighting, alpha blending at 334 FPS on RTX 4070 Ti.*

## Current State

**21 milestones complete.** Loads full Fallout: New Vegas interior/exterior cells and Skyrim SE meshes from real game data. Plays .kf animations with full scene graph hierarchy.

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
| Vulkan renderer with depth, directional lighting, alpha blending, GlobalTransform | Working |
| ESM parser (CELL, REFR, STAT, MSTT, FURN, DOOR, ACTI, CONT, LIGH + 23 record types) | Working |
| NIF parser (37 block types, Z-up → Y-up, NifVariant for 8 games) | Working |
| DDS texture loading (BC1/BC3/BC5 + DX10, mipmaps, per-mesh binding) | Working |
| BSA v104 + v105 archive reader (zlib + LZ4) | Working |
| Interior + exterior cell loading with placed object transforms | Working |
| Animation playback (.kf files, linear/Hermite/TBC interpolation, 8 controller types) | Working |
| Scene graph hierarchy with transform propagation | Working |
| Skyrim SE NIF support (BSTriShape, BSLightingShader, packed vertices) | Working |
| Scaleform/SWF UI system (Ruffle integration, offscreen wgpu→Vulkan bridge) | Working |
| Fly camera (WASD + mouse look) | Working |
| Debug diagnostics (--debug, --cmd, console commands) | Working |
| Plugin system with stable Form IDs, conflict resolution | Working |
| ECS-native scripting (events, timers) | Working |
| 282 unit tests | Passing |

## Architecture

```
byroredux/            Binary — game loop, cell loader, fly camera, animation system
crates/
  core/                    ECS, math (glam), animation engine, types, string interning, form IDs
  renderer/                Vulkan graphics via ash + gpu-allocator
  plugin/                  Plugin system, ESM parser, manifests, conflict resolution
  nif/                     NIF file parser + animation importer (.nif/.kf)
  bsa/                     BSA archive reader (v104 + v105 with LZ4)
  ui/                      Scaleform/SWF UI system (Ruffle integration)
  scripting/               ECS-native scripting (events, timers)
  platform/                Windowing via winit (Linux-first)
  cxx-bridge/              C++ interop via cxx
```

### ECS Design

Components declare their own storage backend — no runtime dispatch:

- **SparseSetStorage** — O(1) insert/remove via swap-remove. Default for gameplay data.
- **PackedStorage** — Sorted by entity ID, cache-friendly iteration. Opt-in for hot-path components.

World wraps each storage in `RwLock` so query methods take `&self`, enabling concurrent reads across different component types. Multi-component queries acquire locks in `TypeId` order to prevent deadlocks.

### Renderer

Full Vulkan initialization chain via `ash`. No shortcuts:

Instance + validation layers, debug messenger, surface, physical/logical device, swapchain, render pass, dual graphics pipelines (opaque + alpha-blended) with push constants, framebuffers, command pool/buffers, GPU memory allocation via `gpu-allocator`, per-image synchronization, DDS texture upload (BC-compressed with mipmaps), per-mesh descriptor set binding.

### Asset Pipeline

ESM files are parsed for CELL/REFR/STAT records (23 record types) to locate placed objects with their positions and rotations. NIF files are parsed with version-aware binary reading (NifVariant detection for 8 game variants), scene graph hierarchy preserved as Parent/Children entities with local transforms. DDS textures extracted from BSA archives, uploaded to Vulkan as BC-compressed images with full mipmap chains.

### Animation

.kf files (Gamebryo animation format) are parsed and imported as `AnimationClip` resources. The interpolation engine supports linear, cubic Hermite (quadratic tangents), and Kochanek-Bartels (TBC) splines for position/rotation/scale. 8 controller types: transform, material color, alpha, visibility, UV transform, and shader property controllers. Cycle modes: clamp, loop, reverse (ping-pong). Transform propagation system computes world-space matrices from local transforms each frame.

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
cargo run                          # Demo scene with spinning cube
cargo run -- path/to/mesh.nif      # Render a loose NIF file
cargo run -- mesh.nif --kf anim.kf # Play animation on a mesh
cargo run -- --esm FalloutNV.esm \
             --cell CellEditorID \
             --bsa "Fallout - Meshes.bsa" \
             --textures-bsa "Fallout - Textures.bsa"  # Load an interior cell
cargo run -- --swf menu.swf        # Render a Scaleform SWF menu
cargo run -- --debug               # Show FPS/entity stats in title bar
cargo run -- --cmd "help"          # Run a console command and exit
cargo test                         # Run all 282 tests
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

## Roadmap

| Phase | Status | Milestone |
|-------|--------|-----------|
| M1–M8 | Done | Graphics foundation, ECS, plugin system, depth, texturing |
| M9–M11 | Done | NIF parser (37 block types), NIF-to-ECS import, BSA v104+v105 reader |
| M12–M13 | Done | ECS-native scripting, directional lighting |
| M14–M16 | Done | DDS textures, debug diagnostics, ESM parser + cell loading |
| M17–M18 | Done | Coordinate system fix, Skyrim SE NIF support (BSTriShape, BSLightingShader) |
| M19 | Done | Full cell loading — all renderable record types, WRLD exterior cells |
| M20 | Done | Scaleform/SWF UI system (Ruffle integration) |
| M21 | Done | Animation playback — .kf parsing, interpolation engine, 8 controller types |
| M22 | Next | RT-first multi-light system (Vulkan ray tracing + rasterized fallback) |

See [ROADMAP.md](ROADMAP.md) for the full roadmap with details, known issues, and game compatibility.

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
| nalgebra | SVD for degenerate NIF matrix repair |
| lz4_flex | BSA v105 decompression |
| flate2 | BSA v104/ESM zlib decompression |
| image | Texture format support |
| ruffle_core | SWF/Flash menu rendering |
| cxx | C++ interop |

## License

MIT
