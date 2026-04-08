# ByroRedux

A clean rebuild of the Gamebryo/Creation engine lineage in **Rust + C++**, using **Vulkan** for rendering. Linux-first, with the long-term goal of loading and running content from Gamebryo/Creation-era games (Oblivion, Fallout 3, New Vegas, Skyrim, Fallout 4, Fallout 76, Starfield).

Not a port — a ground-up rebuild that understands the legacy architecture and builds modern equivalents.

![*Prospector Saloon* from Fallout: New Vegas, rendered in ByroRedux](docs/screenshots/prospector-saloon.png)

*The Prospector Saloon (Goodsprings) loaded from FalloutNV.esm — 789 entities, RT multi-light with shadows, cell interior XCLL lighting at 85 FPS on RTX 4070 Ti.*

## Current State

**23 milestones complete (M1–M22, M24 Phase 1, M26, M28 Phase 1), N23 NIF parser overhaul complete (10/10), N26 Oblivion coverage audit done.** Loads cells from every Bethesda Gamebryo/Creation game and renders them with RT shadows. NIF parser hits **100% on every supported game** across the full archive sweeps. Full ESM record parser extracts items, NPCs, factions, and supporting metadata. Rapier3D physics simulates the loaded cell: the player capsule collides with world geometry, dynamic clutter falls under gravity. Per-mesh NiLight sources (Oblivion torches, candles, magic FX) now contribute to the RT light buffer alongside cell XCLL ambient.

```bash
# FNV interior cell with full lighting
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

# Per-game NIF parse rate sweep (requires game data)
cargo test -p byroredux-nif --release --test parse_real_nifs -- --ignored
```

Press **Escape** to capture mouse, then **WASD** + mouse to fly around. **Space/Shift** for up/down, **Ctrl** for speed boost.

## Game Compatibility

Every supported Bethesda game parses its full mesh archive without errors:

| Game              | Archive format | NIF parse rate     | Cells | Notes                                      |
|-------------------|----------------|--------------------|-------|--------------------------------------------|
| Oblivion          | BSA v103       | **100%** (8,032)   | —     | Decompression + pre-Gamebryo header fixes  |
| Fallout 3         | BSA v104       | **100%** (10,989)  | ✓     | Megaton interior, full Wasteland           |
| Fallout New Vegas | BSA v104       | **100%** (14,881)  | ✓     | Prospector Saloon, exterior 3×3 grid       |
| Skyrim SE         | BSA v105 (LZ4) | **100%** (18,862)  | —     | Full mesh archive coverage                 |
| Fallout 4         | BA2 v8         | **100%** (34,995)  | —     | BA2 GNRL + DX10 textures                   |
| Fallout 76        | BA2 v1         | **100%** (58,469)  | —     | FO76 stopcond shader paths                 |
| Starfield         | BA2 v2         | **100%** (31,058)  | —     | 32-byte header extension                   |

**Total: 177,286 NIFs parse cleanly across the entire Bethesda lineage.**
See [Game Compatibility](docs/engine/game-compatibility.md) for the per-game architecture details.

## Capability Matrix

| Feature | Status |
|---------|--------|
| ECS with pluggable storage (SparseSet + Packed), hierarchy (Parent/Children) | Working |
| Vulkan RT renderer with multi-light SSBO, ray query shadows, cell XCLL lighting | Working |
| Per-mesh `NiLight` sources (ambient / directional / point / spot) → GpuLight | Working |
| NIF parser (~210 block types) — Oblivion through Starfield, 100% per-game success | Working |
| Rapier3D physics simulation — collision from NIF bhk chain, fixed 60 Hz substep, dynamic-capsule player body | Working |
| BSA reader (v103/v104/v105) — Oblivion through Skyrim SE | Working |
| BA2 reader (v1/v2/v3/v7/v8) — FO4, FO76, Starfield, GNRL + DX10 with reconstructed DDS headers | Working |
| ESM/ESP parser — cells, statics, items, NPCs, factions, leveled lists, globals (10+ record categories) | Working |
| Interior + exterior cell loading with placed object transforms | Working |
| DDS texture loading (BC1/BC3/BC5 + DX10, mipmaps, shared sampler cache) | Working |
| Animation playback (.kf files, linear/Hermite/TBC, 8 controller types, blending stack) | Working |
| NiControllerManager embedded animation discovery + text key events as ECS markers | Working |
| Scene graph hierarchy (Parent/Children) with transform propagation | Working |
| Skyrim SE NIF support (BSTriShape, BSLightingShader, packed vertices) | Working |
| FO76/Starfield shader stopcond, CRC32 flag arrays, Luminance/Translucency | Working |
| Scaleform/SWF UI (Ruffle integration, deferred texture updates) | Working |
| Pipeline cache with disk persistence | Working |
| Fly camera (WASD + mouse look) | Working |
| Plugin system with stable Form IDs, conflict resolution | Working |
| ECS-native scripting (events, timers) | Working |
| Material component (emissive, specular, glossiness, UV, normal map) | Working |
| Collision import (Havok shapes → ECS, compressed mesh for Skyrim) | Working |
| Per-game integration test infrastructure with 95% parse rate threshold | Working |

## Architecture

```
byroredux/                 Binary — game loop, cell loader, fly camera, animation system
crates/
  core/                    ECS, math (glam), animation engine, types, string interning, form IDs
  renderer/                Vulkan graphics via ash + gpu-allocator (RT extensions)
  plugin/                  Plugin system + ESM/ESP parser (cells, items, NPCs, factions, ...)
  nif/                     NIF file parser (~210 block types) + animation importer
  bsa/                     BSA + BA2 archive readers (Oblivion → Starfield)
  physics/                 Rapier3D bridge (M28 Phase 1) — NIF collision → ECS → stepper
  ui/                      Scaleform/SWF UI system (Ruffle integration)
  scripting/               ECS-native scripting (events, timers)
  platform/                Windowing via winit (Linux-first)
  cxx-bridge/              C++ interop via cxx
```

See [Architecture Overview](docs/engine/architecture.md) for design principles, the crate dependency graph, and a tour of each subsystem.

### ECS Design

Components declare their own storage backend — no runtime dispatch:

- **SparseSetStorage** — O(1) insert/remove via swap-remove. Default for gameplay data.
- **PackedStorage** — Sorted by entity ID, cache-friendly iteration. Opt-in for hot-path components.

World wraps each storage in `RwLock` so query methods take `&self`, enabling concurrent reads across different component types. Multi-component queries acquire locks in `TypeId` order to prevent deadlocks. Zero unsafe blocks in the ECS module — `ComponentRef` guard pattern ensures sound lifetime management.

### Renderer

Vulkan 1.3 via `ash` with RT ray query extensions:

- Full initialization chain with validation layers in debug builds
- RT acceleration structures (BLAS per mesh + TLAS per frame) with `DEVICE_LOCAL` memory
- Multi-light SSBO with point/spot/directional lights and RT shadow rays
- Pipeline cache with disk persistence (10–50 ms cold → <1 ms warm)
- Shared `VkSampler` across all textures
- Per-image semaphore synchronization with HOST→AS_BUILD memory barriers
- Deferred texture destruction for stall-free dynamic UI updates
- Atomic swapchain handoff on resize
- Proper depth format querying with fallback chain (D32→D32S8→D24S8→D16)
- Backface culling with confirmed NIF/D3D CW winding convention

See [Vulkan Renderer](docs/engine/renderer.md) for the per-module breakdown and the [Lighting System](docs/engine/lighting-from-cells.md) doc for cell-based lighting.

### Asset Pipeline

ESM files parsed for cell, static, item, NPC, faction, and supporting records. NIF files parsed with version-aware binary reading (`NifVariant` for 8 game variants, 186 registered block types). Full coverage: ~48 particle system types, `bhkCompressedMeshShape` for Skyrim collision, FO4 half-float vertices + shader wetness params, all 6 skinning blocks, 30+ Havok collision shapes, FO76/Starfield shader stopcond + CRC32 flag arrays. Collision import to ECS with Havok→engine coordinate conversion. Scene graph hierarchy preserved as Parent/Children entities. Single-pass material property extraction into `Material` ECS component. DDS textures from BSA v103/v104/v105 archives and BA2 v1/2/7 DX10 archives with reconstructed headers. `StagingPool` for reusable GPU upload buffers. `NiControllerManager` embedded animation discovery with text key event emission.

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
cargo test                         # All workspace tests
cargo test -p byroredux-nif --release --test parse_real_nifs -- --ignored
                                   # Per-game NIF parse rate sweeps (needs game data)
```

### Per-Game Data Paths

The integration tests resolve game data via environment variables, falling back to canonical Steam install paths:

```
BYROREDUX_OBLIVION_DATA   /mnt/data/SteamLibrary/.../Oblivion/Data
BYROREDUX_FO3_DATA        .../Fallout 3 goty/Data
BYROREDUX_FNV_DATA        .../Fallout New Vegas/Data
BYROREDUX_SKYRIMSE_DATA   .../Skyrim Special Edition/Data
BYROREDUX_FO4_DATA        .../Fallout 4/Data
BYROREDUX_FO76_DATA       .../Fallout76/Data
BYROREDUX_STARFIELD_DATA  .../Starfield/Data
```

### Shader Compilation

Shaders are pre-compiled to SPIR-V and included via `include_bytes!`:

```bash
cd crates/renderer/shaders
glslangValidator -V triangle.vert -o triangle.vert.spv
glslangValidator -V triangle.frag -o triangle.frag.spv
```

## Documentation

### Engine
- [Engine Index](docs/engine/index.md) — gateway to every doc
- [Architecture Overview](docs/engine/architecture.md) — design principles, crate graph
- [ECS](docs/engine/ecs.md) — components, storage, queries, scheduler
- [Vulkan Renderer](docs/engine/renderer.md) — RT pipeline, multi-light, BLAS/TLAS
- [NIF Parser](docs/engine/nif-parser.md) — ~210 block types, version handling, parse-rate matrix
- [Archives (BSA + BA2)](docs/engine/archives.md) — BSA v103/104/105 and BA2 v1/2/3/7/8
- [ESM Records](docs/engine/esm-records.md) — cell loading + structured record extraction
- [Animation](docs/engine/animation.md) — keyframe pipeline, controllers, blending stack
- [Physics](docs/engine/physics.md) — Rapier3D bridge, collision pipeline, player body
- [Asset Pipeline](docs/engine/asset-pipeline.md) — texture provider, mesh cache, NIF→ECS
- [UI System](docs/engine/ui.md) — Scaleform/SWF via Ruffle
- [Game Compatibility](docs/engine/game-compatibility.md) — 7-game parse rate matrix
- [Cell Lighting](docs/engine/lighting-from-cells.md) — XCLL extraction, RT integration
- [Coordinate System](docs/engine/coordinate-system.md) — Z-up→Y-up, CW rotations
- [Game Loop](docs/engine/game-loop.md) — winit, frame loop, cell loading
- [Testing](docs/engine/testing.md) — unit + integration test inventory
- [Dependencies](docs/engine/dependencies.md) — workspace and per-crate
- [String Interning](docs/engine/string-interning.md), [C++ Interop](docs/engine/cxx-interop.md), [Platform](docs/engine/platform.md), [Scripting](docs/engine/scripting.md)

### Legacy reference
- [Gamebryo 2.3 Architecture](docs/legacy/gamebryo-2.3-architecture.md) — class hierarchy, NIF format, compatibility mapping
- [Key Source Files](docs/legacy/key-files.md) — paths to critical headers by subsystem
- [API Deep Dive](docs/legacy/api-deep-dive.md) — `NiObject`, `NiAVObject`, `NiStream`, `NiProperty`, `NiTransform`
- [Papyrus API Reference](docs/legacy/papyrus-api-reference.md) — what the script runtime needs to mirror
- [Creation Engine UI](docs/legacy/creation-engine-ui.md) — Scaleform menu architecture

### Project state
- [Development Roadmap](ROADMAP.md) — milestones, deferred work, achievement matrix

## Stats

| Metric                                | Value          |
|---------------------------------------|----------------|
| Rust source files                     | 149            |
| Lines of Rust                         | ~39,600        |
| Unit tests passing                    | 396            |
| Integration tests (`#[ignore]`'d)     | 22             |
| NIFs in per-game integration sweeps   | 177,286        |
| Per-game parse success rate           | 100% (7 games) |
| Workspace crates                      | 11             |

## Dependencies

| Crate           | Purpose                                       |
|-----------------|-----------------------------------------------|
| ash             | Raw Vulkan bindings                           |
| ash-window      | Surface creation from window handles          |
| gpu-allocator   | Vulkan memory allocation                      |
| winit           | Cross-platform windowing                      |
| glam            | Linear algebra (Vec/Mat/Quat)                 |
| nalgebra        | SVD for degenerate rotation repair            |
| string-interner | O(1) string equality                          |
| flate2          | Zlib decompression for BSA + BA2              |
| lz4_flex        | LZ4 frame decompression for BSA v105          |
| image           | PNG / image loading                           |
| rapier3d        | Physics simulation (M28 Phase 1)              |
| serde / toml    | Plugin manifest serialization                 |
| uuid / semver   | Plugin identity and version constraints       |
| anyhow / thiserror | Error handling                             |
| cxx             | C++ interop                                   |
| log / env_logger | Structured logging                           |

See [Dependencies](docs/engine/dependencies.md) for the full per-crate breakdown.

## License

MIT
