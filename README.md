# ByroRedux

A clean rebuild of the Gamebryo/Creation engine lineage in **Rust + C++**, using **Vulkan** for rendering. Linux-first, with the long-term goal of loading and running content from Gamebryo/Creation-era games (Oblivion, Fallout 3, New Vegas, Skyrim, Fallout 4, Fallout 76, Starfield).

Not a port — a ground-up rebuild that understands the legacy architecture and builds modern equivalents.

![*Anvil Heinrich Oaken Halls* from The Elder Scrolls IV: Oblivion, rendered in ByroRedux](docs/screenshots/anvil-oaken-halls.png)

*Anvil Heinrich Oaken Halls House (Oblivion) loaded directly from `Oblivion.esm` + `Oblivion - Meshes.bsa` + `Oblivion - Textures - Compressed.bsa` — 379 entities, 376 meshes, 104 DDS textures, 12 lights (cell XCLL ambient + per-mesh NiLight torches), RT multi-light with ray query shadows at ~100 FPS on RTX 4070 Ti.*

![*Prospector Saloon* from Fallout: New Vegas, rendered in ByroRedux](docs/screenshots/prospector-saloon.png)

*The Prospector Saloon (Goodsprings) loaded from `FalloutNV.esm` — 789 entities, RT multi-light with shadows, cell interior XCLL lighting at 85 FPS on RTX 4070 Ti.*

## Current State

**30+ milestones complete**, including RT performance at scale (M31), streaming RIS direct lighting (M31.5), landscape terrain (M32), exterior sun lighting (M34), BLAS compaction (M36), temporal antialiasing (M37.5), and Papyrus language parser (M30). Loads and renders both **interior and exterior cells** directly from unmodified Bethesda game data — interior cells with placed objects, lighting, and RT shadows; exterior cells with 3x3 grids and heightmap terrain meshes with texture splatting. NIF parser hits **100% on every supported game** across 177,286 NIFs. RT renderer features streaming weighted reservoir shadow sampling (8 reservoirs / fragment), instanced draw batching, BLAS LRU eviction + compaction, SVGF temporal denoiser, TAA with Halton jitter + YCoCg neighborhood clamp, and distance-based ray fallback. Rapier3D physics, ESM record parsing (items, NPCs, factions, FO4 SCOL/MOVS/PKIN/TXST plus SCOL body + XCLW water + XESP gating, CREA/LVLC/SCPT/ACRE, CLMT `TNAM` weather hours, Skyrim XCLL directional-ambient cube + specular + fresnel), full 8-slot TXST extraction + BSShaderTextureSet parallax/env routed to GpuInstance with POM, BGEM `material_path` captured on both NiTriShape and BsTriShape, skeletal skinning pipeline, KFM animation state machines with BSAnimNote IK hints, end-to-end CPU particle system for torches/FX, process-lifetime NIF import cache, persistent BSA/BA2 file handles, pipeline cache threaded through every create site with disk persistence, SPIR-V reflection cross-checks descriptor layouts against shader declarations at pipeline create time, debug CLI with BGSM material diagnostics.

```bash
# Oblivion interior cell with XCLL lighting + per-mesh NiLight torches
cargo run --release -- --esm Oblivion.esm --cell AnvilHeinrichOakenHallsHouse \
             --bsa "Oblivion - Meshes.bsa" \
             --textures-bsa "Oblivion - Textures - Compressed.bsa"

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

Every supported Bethesda game parses its full mesh archive without errors. **Oblivion** and **Fallout New Vegas** additionally load and render real interior cells end-to-end from unmodified game data.

| Game              | Archive format | NIF parse rate     | Cells rendering | Notes                                                 |
|-------------------|----------------|--------------------|-----------------|-------------------------------------------------------|
| Oblivion          | BSA v103       | **100%** (8,032)   | ✓ Interior      | 20-byte TES4 headers, N26 block audit, Heinrich Halls |
| Fallout 3         | BSA v104       | **100%** (10,989)  | ✓ Int + wired ext | Megaton interior (929 REFRs); exterior via `--grid 0,0` after #444 (fresh bench #457) |
| Fallout New Vegas | BSA v104       | **100%** (14,881)  | ✓ Int + 3×3 ext | Prospector Saloon, exterior 3×3 grid                  |
| Skyrim SE         | BSA v105 (LZ4) | **100%** (18,862)  | —               | Full mesh archive coverage                            |
| Fallout 4         | BA2 v1/v7/v8   | **100%** (34,995)  | ✓ Architecture  | BA2 GNRL + DX10 textures, SCOL/MOVS/PKIN/TXST records |
| Fallout 76        | BA2 v1         | **100%** (58,469)  | —               | FO76 stopcond shader paths                            |
| Starfield         | BA2 v2/v3      | **100%** (31,058)  | —               | GNRL + DX10 textures, LZ4 compression, ~128K textures |

**Total: 177,286 NIFs parse cleanly across the entire Bethesda lineage.**
See [Game Compatibility](docs/engine/game-compatibility.md) for the per-game architecture details.

## Capability Matrix

| Feature | Status |
|---------|--------|
| ECS with pluggable storage (SparseSet + Packed), hierarchy (Parent/Children) | Working |
| Vulkan RT renderer with multi-light SSBO, ray query shadows, streaming RIS (8 reservoirs/fragment), SVGF denoiser, TAA | Working |
| BLAS compaction + LRU eviction, batched builds, TLAS frustum culling, deferred SSBO rebuild | Working |
| 16× anisotropic filtering on the shared sampler when the device exposes it | Working |
| Per-mesh `NiLight` sources (ambient / directional / point / spot) → GpuLight | Working |
| Skeletal skinning end-to-end: `SkinnedMesh` ECS component, bone-palette SSBO (4096 slots), shader skinning | Working |
| NIF parser (~210 block types) — Oblivion through Starfield, 100% per-game success | Working |
| KFM animation state-machine parser (Gamebryo 1.2.0.0 → 2.2.0.0 binary format) | Working |
| End-to-end cell rendering from unmodified game data (FNV interior + exterior, FO3 interior) | Working |
| Landscape terrain from LAND heightmap records with LTEX/TXST texture splatting | Working |
| Instanced draw batching — identical meshes merge into single draw calls | Working |
| BLAS lifecycle management — batched builds, LRU eviction, TLAS culling | Working |
| Alpha test with per-material comparison function (8 Gamebryo TestFunction modes) | Working |
| Dark map / multiplicative lightmap (NiTexturingProperty slot 1) | Working |
| Rapier3D physics simulation — collision from NIF bhk chain, fixed 60 Hz substep | Working (static/dynamic bodies); kinematic character controller → M28.5 |
| BSA reader (v103/v104/v105) — Oblivion through Skyrim SE | Working |
| BA2 reader (v1/v2/v3/v7/v8) — FO4, FO76, Starfield, GNRL + DX10 with reconstructed DDS headers, zlib + LZ4 | Working |
| ESM/ESP parser — cells, statics, items, NPCs, factions, leveled lists, globals, CREA/LVLC, SCPT (pre-Papyrus bytecode), ACRE placements | Working |
| FO4 architecture placements — SCOL body (ONAM/DATA child list), MOVS, PKIN, all 8 TXST slots | Working |
| CELL XCLW water plane height + REFR XESP default-disabled gating | Working |
| CLMT `TNAM` sunrise/sunset/volatility hours threaded through `weather_system` | Working |
| Skyrim XCLL directional-ambient cube + specular + fresnel | Working |
| Worldspace auto-pick + FormID mod-index remap when loading cells by editor ID | Working |
| BSShaderTextureSet parallax + env slots routed to GpuInstance with POM gating | Working |
| BSShaderPPLighting / BSLightingShader glow/detail/gloss (slots 3/4/5), NiZBufferProperty z_test/z_write/z_function via extended dynamic state | Working |
| SPIR-V reflection cross-checks every descriptor-set layout against shader declarations at pipeline create time | Working |
| Bindless texture array sized from device limit with hard-fail on overflow | Working |
| BSAnimNote / BSAnimNotes parsed — IK hints surfaced on `AnimationClip` | Working |
| Interior + exterior cell loading with placed objects, 3x3 exterior grid | Working |
| End-to-end CPU particle system — torches, magic FX render from NIF particle data | Working |
| Process-lifetime NIF import cache + long-lived BSA/BA2 file handles across cell extracts | Working |
| VkPipelineCache threaded through every pipeline create site with disk persistence | Working |
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
| Papyrus language parser (`.psc` source → typed AST, Phase 1: lexer + expressions) | Working |
| Debug CLI (`byro-dbg`) — live ECS inspection, component queries, screenshots over TCP | Working |
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
  papyrus/                 Papyrus language parser (.psc source → typed AST)
  debug-protocol/          Wire types + component registry for debug CLI
  debug-server/            TCP debug server (Late-stage exclusive system)
  platform/                Windowing via winit (Linux-first)
  cxx-bridge/              C++ interop via cxx
tools/
  byro-dbg/                Standalone debug CLI — live ECS inspection over TCP
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
- RT acceleration structures (BLAS per mesh + TLAS per frame) with `DEVICE_LOCAL` memory, LRU eviction, ALLOW_COMPACTION + query-based compact copy, batched builds
- Multi-light SSBO with point/spot/directional lights and streaming weighted reservoir sampling (8 reservoirs/fragment, unbiased W = resWSum / (K·w_sel) clamped at 64×)
- TAA compute pass with Halton(2,3) jitter, Catmull-Rom history resample, YCoCg neighborhood variance clamp (γ=1.25), mesh-id disocclusion
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
cargo run -p byro-dbg              # Connect debug CLI to running engine
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

### Debug CLI

The engine includes a built-in TCP debug server (port 9876, enabled by default) and a standalone CLI tool for live inspection:

```bash
# In one terminal: run the engine
cargo run --release -- --esm FalloutNV.esm --cell GSProspectorSaloonInterior \
             --bsa "Fallout - Meshes.bsa" --textures-bsa "Fallout - Textures.bsa"

# In another terminal: connect the debugger
cargo run -p byro-dbg
```

```
byro> stats
FPS: 60.2 (avg 59.8) | Frame: 16.61ms | Entities: 789 | Meshes: 342 | Textures: 128 | Draws: 286

byro> find("TorchSconce01")
  Entity 142 "TorchSconce01"

byro> 142.Transform
{ "translation": [1024.0, 512.0, 128.0], "rotation": [0, 0, 0, 1], "scale": 1.0 }

byro> 142.LightSource
{ "radius": 512.0, "color": [1.0, 0.8, 0.6], "flags": 0 }

byro> entities(LightSource)
  Entity 10 "CandleFlame"
  Entity 142 "TorchSconce01"
(2 entities)

byro> screenshot /tmp/debug.png
Screenshot saved: /tmp/debug.png
```

Uses the Papyrus expression parser as query language — member access chains (`42.Transform.translation.x`), function calls (`find("name")`), entity listing (`entities(Component)`), and screenshot capture. Zero per-frame cost when no debugger is connected. Disable with `cargo build --no-default-features`.

See [Debug CLI](docs/engine/debug-cli.md) for the full protocol reference, architecture, and component registry.

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
- [Papyrus Parser](docs/engine/papyrus-parser.md) — `.psc` lexer, AST, expression parser (Phase 1)
- [Debug CLI](docs/engine/debug-cli.md) — live ECS inspection, expression queries, screenshots
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
| Rust source files                     | 188            |
| Lines of Rust                         | ~81,000        |
| Unit tests passing                    | 867            |
| Integration tests (`#[ignore]`'d)     | 32             |
| NIFs in per-game integration sweeps   | 177,286        |
| Per-game parse success rate           | 100% (7 games) |
| Workspace members                     | 15 (13 engine crates + binary + debug CLI) |

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
| lz4_flex        | LZ4 decompression (frame for BSA v105, block for BA2 v3) |
| image           | PNG / image loading                           |
| rapier3d        | Physics simulation (M28 Phase 1)              |
| logos           | Lexer generator for Papyrus parser             |
| serde / serde_json / toml | Plugin manifests, debug protocol serialization |
| uuid / semver   | Plugin identity and version constraints       |
| anyhow / thiserror | Error handling                             |
| cxx             | C++ interop                                   |
| log / env_logger | Structured logging                           |

See [Dependencies](docs/engine/dependencies.md) for the full per-crate breakdown.

## Acknowledgements

- [**nifxml**](https://github.com/niftools/nifxml) — the NifTools project's
  machine-readable NIF format specification. ByroRedux's NIF parser is written
  directly against nifxml's block definitions, version gates, and field
  conditions. Without that decades-long community reverse-engineering effort,
  supporting seven Gamebryo/Creation-era games end-to-end would not be
  tractable. Thank you to every contributor who chipped away at that file.
- [**Ruffle**](https://ruffle.rs) — the open-source Flash Player emulator.
  ByroRedux's UI layer embeds `ruffle_core` + `ruffle_render_wgpu` to render
  the Scaleform/SWF menus Bethesda shipped with every Creation Engine title.
  Shoutout to the Ruffle team for keeping the Flash runtime alive and for
  maintaining a Rust-native API clean enough to drop into an engine.

## License

MIT
