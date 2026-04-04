# ByroRedux

A clean rebuild of the Gamebryo/Creation engine lineage in **Rust + C++**, using **Vulkan** for rendering. Linux-first, with the long-term goal of loading and running content from Gamebryo/Creation-era games (Oblivion, Fallout 3, New Vegas, Skyrim).

Not a port — a ground-up rebuild that understands the legacy architecture and builds modern equivalents.

![*Prospector Saloon* from Fallout: New Vegas, rendered in ByroRedux](docs/screenshots/prospector-saloon.png)

*The Prospector Saloon (Goodsprings) loaded from FalloutNV.esm — 789 entities, RT multi-light with shadows, cell interior XCLL lighting at 85 FPS on RTX 4070 Ti.*

## Current State

**22 milestones complete (M1–M22).** Loads full Fallout: New Vegas interior/exterior cells and Skyrim SE meshes with RT ray-traced shadows. Multi-light rendering with cell ambient/directional from ESM. Animation playback with scene graph hierarchy. Currently overhauling the NIF parser for Oblivion-through-Starfield support (N23 series).

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
| NIF parser (49 block type names, trait hierarchy, NifVariant for 8 games, decal detection) | Working |
| DDS texture loading (BC1/BC3/BC5 + DX10, mipmaps, per-mesh binding) | Working |
| BSA v103 + v104 + v105 archive reader (Oblivion/FO3/FNV/Skyrim LE/SE) | Working |
| Interior + exterior cell loading with placed object transforms | Working |
| Animation playback (.kf files, linear/Hermite/TBC, 8 controller types, blending stack) | Working |
| Scene graph hierarchy (Parent/Children) with transform propagation | Working |
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

World wraps each storage in `RwLock` so query methods take `&self`, enabling concurrent reads across different component types. Multi-component queries acquire locks in `TypeId` order to prevent deadlocks.

### Renderer

Vulkan 1.3 via `ash` with RT ray query extensions:

Instance + validation layers, debug messenger, surface, physical/logical device (with VK_KHR_acceleration_structure + VK_KHR_ray_query when available), swapchain, render pass, 4 graphics pipelines (opaque/alpha × culled/two-sided) with dynamic depth bias, per-frame SSBO for light array + UBO for camera/ambient, BLAS per mesh + TLAS rebuilt per frame, push constants (viewProj + model), DDS texture upload (BC-compressed with mipmaps), per-mesh descriptor set binding. Graceful fallback on non-RT GPUs.

### Asset Pipeline

ESM files are parsed for CELL/REFR/STAT records (23 record types) to locate placed objects with positions and rotations. Interior cell lighting parsed from XCLL subrecords (ambient + directional). NIF files parsed with version-aware binary reading (NifVariant detection for 8 game variants, 48 block types), scene graph hierarchy preserved as Parent/Children entities with local transforms. Decal geometry detected via NIF shader flags for proper depth bias. DDS textures extracted from BSA v103/v104/v105 archives, uploaded to Vulkan as BC-compressed images with full mipmap chains.

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
| M9–M11 | Done | NIF parser, NIF-to-ECS import, BSA v103/v104/v105 reader |
| M12–M16 | Done | Scripting, lighting, DDS textures, ESM parser, cell loading |
| M17–M18 | Done | Coordinate system fix, Skyrim SE NIF (BSTriShape, BSLightingShader) |
| M19–M20 | Done | Full cell loading (WRLD exterior), Scaleform/SWF UI (Ruffle) |
| M21 | Done | Animation playback — .kf parsing, interpolation, blending stack |
| M22 | Done | RT multi-light — SSBO lights, ray query shadows, cell XCLL lighting |
| **N23** | **Active** | **NIF parser overhaul — Oblivion through Starfield (10 sub-milestones)** |

**Current focus: N23 (NIF correctness).** N23.1 done (trait hierarchy, 7 FNV parser bugs fixed via audit). Target: ~130 block types across 8 games, up from 49.

See [ROADMAP.md](ROADMAP.md) for the full roadmap with N23 sub-milestones and game compatibility.

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
