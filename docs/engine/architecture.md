# ByroRedux — Architecture Overview

ByroRedux is a clean rebuild of the Gamebryo and Creation engine lineage,
built from scratch in Rust and C++ using Vulkan. The goal is a modern engine
with Rust's safety guarantees that can load and render content from every
Bethesda Gamebryo/Creation game (Oblivion through Starfield).

## Workspace Structure

```
byroredux/
├── Cargo.toml                 Workspace root
├── byroredux/                 Binary crate — game loop, scene setup, cell loader,
│                              fly camera, animation/transform/render systems
├── crates/
│   ├── core/                  ECS, math (glam), animation engine, types,
│   │                          string interning, form IDs, components/resources
│   ├── plugin/                Plugin system, ESM/ESP parser (cell + records),
│   │                          manifests, conflict resolution, legacy bridge
│   ├── nif/                   NIF file parser (186 block types) + animation
│   │                          importer, scene import to ECS-friendly meshes
│   ├── bsa/                   BSA + BA2 archive readers for every Bethesda game
│   │                          (Oblivion through Starfield)
│   ├── physics/               Rapier3D-backed physics simulation, consumes
│   │                          the collision data the NIF importer emits
│   ├── renderer/              Vulkan graphics via ash + gpu-allocator, with RT
│   │                          extensions (VK_KHR_ray_query)
│   ├── ui/                    Scaleform/SWF UI system (Ruffle integration)
│   ├── scripting/             ECS-native scripting (events, timers)
│   ├── platform/              Windowing via winit (Linux-first)
│   └── cxx-bridge/            C++ interop via cxx
└── docs/
    ├── engine/                This documentation
    └── legacy/                Gamebryo 2.3 / Creation Engine analysis
```

See [crates/core/src/ecs/](../../crates/core/src/ecs/) for the ECS implementation,
[crates/nif/src/blocks/](../../crates/nif/src/blocks/) for the NIF block parsers,
and [crates/renderer/src/vulkan/](../../crates/renderer/src/vulkan/) for the
renderer.

## Design Principles

### 1. ECS over scene graph

The legacy Gamebryo engine uses a hierarchical scene graph where `NiAVObject`
is a God Object bundling transforms, properties, bounds, collision, and flags.
Redux decomposes these into independent components: `Transform`,
`GlobalTransform`, `Parent`, `Children`, `MeshHandle`, `Material`,
`LightSource`, `WorldBound`, etc. Systems that only need transforms never
touch collision data, and the renderer never sees AI state.

### 2. Components declare their own storage

Each component type specifies whether it uses `SparseSetStorage` (O(1)
mutation, gameplay data) or `PackedStorage` (sorted, cache-friendly
iteration, hot-path data) via an associated type. There is no runtime
branching on storage layout.

```rust
struct Velocity { ... }
impl Component for Velocity {
    type Storage = SparseSetStorage<Self>;
}
```

### 3. Interior mutability via RwLock

Every component storage and resource is wrapped in `RwLock`. Query methods
take `&self`, not `&mut self`. This enables multiple systems to read
different component types simultaneously, and is the foundation for the
parallel system dispatch milestone (M27, deferred). Multi-component queries
acquire locks in `TypeId` order to prevent deadlocks.

### 4. Soft fail and keep going

The asset parsers — NIF, BSA, BA2, ESM — are designed to recover from
malformed or unfamiliar data instead of aborting the whole load:

- The NIF block walker seeks past a broken block when `block_size` is known
  and inserts an `NiUnknown` placeholder, so a single buggy parser doesn't
  kill an entire mesh archive sweep.
- The NIF header parser returns an empty scene for pre-Gamebryo NetImmerse
  files instead of erroring out.
- The cell-loader walker tolerates truncated final sub-records.

This is what lets the per-game integration tests measure parse rates as
useful telemetry rather than just pass/fail.

### 5. Rust owns the architecture, C++ provides interop

The engine core, ECS, renderer, parsers, and scene loader are all Rust. C++
is available through the `cxx` crate for performance-critical code or
legacy library integration. The FFI boundary is explicit and type-safe.

### 6. Vulkan done properly

No shortcuts in the initialization chain. Validation layers in debug builds,
proper swapchain recreation on resize, sorted lock acquisition to prevent
deadlocks, atomic swapchain handoff during resize, clean teardown in reverse
initialization order. RT acceleration structures live in `DEVICE_LOCAL`
memory with proper HOST→AS_BUILD memory barriers.

See [Vulkan Renderer](renderer.md) for the per-module breakdown.

### 7. Linux-first, multiplatform later

The primary development platform is Linux. Platform abstractions exist in
the `platform` crate to enable future Windows/macOS support without
touching engine internals. The reference development machine is an AMD
Ryzen 9 7950X with an NVIDIA RTX 4070 Ti running Wayland.

## Crate Dependency Graph

```
byroredux (binary)
├── byroredux-core
├── byroredux-renderer
│   ├── byroredux-core
│   └── byroredux-platform
│       └── byroredux-core
├── byroredux-platform
├── byroredux-plugin
│   └── byroredux-core
├── byroredux-nif
│   └── byroredux-core
├── byroredux-bsa
│   └── (no internal deps — leaf)
├── byroredux-physics
│   └── byroredux-core
├── byroredux-ui
│   └── byroredux-core
├── byroredux-scripting
│   └── byroredux-core
└── byroredux-cxx-bridge
    └── (no internal deps — leaf)
```

`byroredux-core` is the spine — every other engine crate depends on it for
ECS types, math, animation, string interning, and form IDs. `byroredux-bsa`
and `byroredux-cxx-bridge` are leaves (no internal deps), which keeps the
asset reader and the C++ bridge testable in isolation. `byroredux-physics`
depends only on `core`, keeping `rapier3d` / `nalgebra` out of every other
crate's build graph.

The integration tests in `crates/nif/tests/parse_real_nifs.rs` add a test-only
dependency on `byroredux-bsa` so a single test binary can walk both BSA and
BA2 archives through a unified `MeshArchive` enum.

## Subsystem map

Where each major capability lives, by milestone group:

| Capability | Crate(s) | Source root |
|---|---|---|
| ECS world / queries / scheduler | `core` | [crates/core/src/ecs/](../../crates/core/src/ecs/) |
| Math types (Vec/Mat/Quat) | `core` | [crates/core/src/math.rs](../../crates/core/src/math.rs) |
| Form IDs and plugin identity | `core` | [crates/core/src/form_id.rs](../../crates/core/src/form_id.rs) |
| Animation engine (clips, players, blending) | `core` | [crates/core/src/animation/](../../crates/core/src/animation/) |
| String interning | `core` | [crates/core/src/string/](../../crates/core/src/string/) |
| Plugin manifests + DataStore | `plugin` | [crates/plugin/src/](../../crates/plugin/src/) |
| ESM cell + record parsing | `plugin` | [crates/plugin/src/esm/](../../crates/plugin/src/esm/) |
| Legacy ESM/ESP/ESL bridge | `plugin` | [crates/plugin/src/legacy/](../../crates/plugin/src/legacy/) |
| NIF binary parser | `nif` | [crates/nif/src/blocks/](../../crates/nif/src/blocks/) |
| NIF→ECS scene import | `nif` | [crates/nif/src/import/](../../crates/nif/src/import/) |
| KF animation import | `nif` | [crates/nif/src/anim/](../../crates/nif/src/anim/) |
| BSA reader (v103/104/105) | `bsa` | [crates/bsa/src/archive.rs](../../crates/bsa/src/archive.rs) |
| BA2 reader (v1/2/3/7/8) | `bsa` | [crates/bsa/src/ba2.rs](../../crates/bsa/src/ba2.rs) |
| Physics world + sync | `physics` | [crates/physics/src/](../../crates/physics/src/) |
| Vulkan context (init, draw, resize) | `renderer` | [crates/renderer/src/vulkan/context/](../../crates/renderer/src/vulkan/context/) |
| Mesh / texture registries | `renderer` | [crates/renderer/src/](../../crates/renderer/src/) |
| RT acceleration structures | `renderer` | [crates/renderer/src/vulkan/acceleration/](../../crates/renderer/src/vulkan/acceleration/) |
| Multi-light SSBO + ray query shadows | `renderer` | [crates/renderer/src/vulkan/scene_buffer/](../../crates/renderer/src/vulkan/scene_buffer/) |
| Ruffle/SWF UI | `ui` | [crates/ui/src/](../../crates/ui/src/) |
| Game loop and cell loader | `byroredux` | [byroredux/src/](../../byroredux/src/) |

## Current State

ByroRedux loads cells from every Bethesda Gamebryo/Creation game and renders
them with RT shadows. The NIF parser hits 100% success across the full mesh
archive sweeps for all seven supported games (177,286 NIFs total). Cells
populate ECS entities with mesh handles, materials, lights, and collision
shapes; transforms compose REFR (placed reference) data with NIF-internal
local transforms via the documented [coordinate system](coordinate-system.md)
pipeline. The ESM parser extracts items, NPCs, factions, leveled lists,
globals, and game settings on top of the existing cell + static extraction.

| Metric                              | Value          |
|-------------------------------------|----------------|
| Rust source files                   | 149            |
| Lines of Rust                       | ~39,600        |
| Workspace crates                    | 11             |
| Unit tests passing                  | 396            |
| Integration tests (`#[ignore]`'d)   | 22             |
| NIFs parsed in integration sweeps   | 177,286        |
| Per-game NIF parse success rate     | 100% (7 games) |
| ESM records parsed from FNV.esm     | 13,684         |

What works today, end-to-end:

- Open a Vulkan window on Linux (Wayland or X11), validation layers in debug
- Load a full FNV interior cell from `FalloutNV.esm` + the BSA, render it
  with RT-shadowed multi-light at 85+ FPS on an RTX 4070 Ti
- Per-mesh NiLight sources contribute to the GpuLight buffer — Oblivion
  torches and candles now light their surroundings
- Simulate physics on the loaded cell via Rapier3D — the player capsule
  collides with world geometry, dynamic clutter falls under gravity
- Load Skyrim SE meshes from BSA v105 (LZ4)
- Load FO4 / FO76 / Starfield meshes from BA2 (v1, v2, v8)
- Play `.kf` animations on named ECS entities with the full controller stack
- Render Scaleform SWF menus via Ruffle as overlay quads
- Walk a 35 k mesh archive in <5 seconds release-mode and assert ≥95% parse rate
