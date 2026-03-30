# ByroRedux

A clean rebuild of the Gamebryo/Creation engine lineage in **Rust + C++**, using **Vulkan** for rendering. Linux-first, with the long-term goal of loading and running content from Gamebryo/Creation-era games (Oblivion, Fallout 3, New Vegas, Skyrim).

Not a port — a ground-up rebuild that understands the legacy architecture and builds modern equivalents.

![Prospector Saloon from Fallout: New Vegas, rendered in ByroRedux](docs/screenshots/prospector-saloon.png)

*The Prospector Saloon (Goodsprings) loaded from FalloutNV.esm — 822 entities, 821 meshes, 128 textures at 284 FPS.*

## Current State

Loads full Fallout: New Vegas interior cells from real game data — ESM records, BSA archives, NIF meshes, DDS textures:

```bash
cargo run -- --esm FalloutNV.esm --cell GSProspectorSaloonInterior \
             --bsa "Fallout - Meshes.bsa" \
             --textures-bsa "Fallout - Textures.bsa" \
             --textures-bsa "Fallout - Textures2.bsa"
```

Press **Escape** to capture mouse, then **WASD** + mouse to fly around. **Space/Shift** for up/down, **Ctrl** for speed boost.

| Feature | Status |
|---------|--------|
| ECS with pluggable storage (SparseSet + Packed) | Working |
| Vulkan renderer with depth buffer, directional lighting, alpha blending | Working |
| ESM parser (CELL, REFR, STAT + 20 record types) | Working |
| NIF parser (15 block types, Z-up → Y-up, NifVariant) | Working |
| DDS texture loading (BC1/BC3, mipmaps, per-mesh binding) | Working |
| BSA v104 archive reader (zlib, embedded file names) | Working |
| Interior cell loading with placed object transforms | Working |
| Fly camera (WASD + mouse look) | Working |
| Debug diagnostics (--debug, --cmd, console commands) | Working |
| Plugin system with stable Form IDs, conflict resolution | Working |
| ECS-native scripting (events, timers) | Working |
| 224 unit tests | Passing |

## Architecture

```
byroredux/            Binary — game loop, cell loader, fly camera
crates/
  core/                    ECS, math (glam), types, string interning, form IDs, console
  renderer/                Vulkan graphics via ash + gpu-allocator
  plugin/                  Plugin system, ESM parser, manifests, conflict resolution
  nif/                     NIF file parser (Gamebryo .nif binary format)
  bsa/                     BSA archive reader (Bethesda Softworks Archive v104)
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

ESM files are parsed for CELL/REFR/STAT records to locate placed objects with their positions and rotations. NIF files are parsed with version-aware binary reading (NifVariant detection for game-specific quirks), scene graphs flattened into ECS entities with coordinate conversion (Gamebryo Z-up → renderer Y-up). DDS textures are extracted from BSA archives and uploaded directly to Vulkan as BC-compressed images with full mipmap chains.

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
cargo run -- --esm FalloutNV.esm \
             --cell CellEditorID \
             --bsa "Fallout - Meshes.bsa" \
             --textures-bsa "Fallout - Textures.bsa"  # Load an interior cell
cargo run -- --debug               # Show FPS/entity stats in title bar
cargo run -- --cmd "help"          # Run a console command and exit
cargo test                         # Run all 224 tests
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
| 1–8 | Done | Graphics foundation, ECS, plugin system, texturing |
| 9–11 | Done | NIF parser, NIF-to-ECS import, BSA reader, real FNV meshes |
| 12–13 | Done | ECS-native scripting, directional lighting |
| 14 | Done | DDS textures, TextureRegistry, per-mesh binding |
| 15 | Done | Debug diagnostics, console commands, --debug/--cmd CLI |
| 16 | Done | ESM parser, interior cell loading, fly camera, alpha blending |
| 17 | Next | RT-first multi-light system (Vulkan ray tracing + rasterized fallback) |
| 18–20 | Planned | Skyrim SE NIF, BSA v105, animation playback |

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
| flate2 | BSA/ESM zlib decompression |
| cxx | C++ interop |

## License

MIT
