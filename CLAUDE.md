# ByroRedux

Clean rebuild of the Gamebryo/Creation engine lineage in Rust + C++ with Vulkan.
Long-term goal: load and render content from Gamebryo/Creation-era games.

## Quick Reference

```bash
cargo check                    # Type check (fast)
cargo test -p byroredux-core    # Run ECS/core tests (111 tests)
cargo test                     # Full workspace tests
cargo run                      # Launch engine (spinning cube demo)
cargo build --release          # Release build
```

### Shader Compilation
```bash
cd crates/renderer/shaders
glslangValidator -V triangle.vert -o triangle.vert.spv
glslangValidator -V triangle.frag -o triangle.frag.spv
```

## Workspace Structure

```
byroredux/              Binary — game loop, scene setup, systems
crates/
  core/                      ECS, math (glam), types, string interning, form IDs
    src/ecs/                 World, Component, Storage, Query, System, Scheduler, Resource
    src/ecs/components/      Transform, GlobalTransform, Parent, Children, Camera, MeshHandle, Name,
                             FormIdComponent, LightSource, AnimatedVisibility/Alpha/Color
    src/ecs/resources.rs     DeltaTime, TotalTime, EngineConfig
    src/animation.rs         Interpolation engine, AnimationClip, AnimationPlayer, AnimationStack,
                             AnimationClipRegistry, RootMotionDelta, blending, sampling
    src/form_id.rs           FormId, PluginId, LocalFormId, FormIdPair, FormIdPool
    src/string/              StringPool, FixedString
  plugin/                    Plugin system — manifests, records, DataStore, conflict resolution
    src/manifest.rs          PluginManifest, TOML parsing
    src/record.rs            Record (component bundles), ErasedComponentData
    src/datastore.rs         DataStore resource, ResolvedRecord, Conflict
    src/resolver.rs          DependencyResolver (DAG), ConflictResolution
    src/legacy/              Legacy ESM/ESP/ESL/ESH bridge
      mod.rs                 LegacyFormId, LegacyLoadOrder
      tes3.rs                Morrowind parser stub
      tes4.rs                Oblivion parser stub
      tes5.rs                Skyrim parser stub
      fo4.rs                 Fallout 4 parser stub
  renderer/                  Vulkan graphics (ash, gpu-allocator, image)
    src/vulkan/              context, pipeline, device, swapchain, sync, allocator, buffer,
                             scene_buffer (SSBO/UBO), acceleration (BLAS/TLAS)
    src/vulkan/texture.rs    Texture upload (RGBA + BC-compressed DDS, staging, layout transitions)
    src/vulkan/dds.rs        DDS header parser (BC1/BC3/BC5, FourCC + DX10 extended, mip sizes)
    src/texture_registry.rs  TextureRegistry (path→handle cache, per-texture descriptor sets)
    src/mesh.rs              MeshRegistry, cube/triangle/quad geometry helpers
    src/vertex.rs            Vertex (position + color + normal + UV), 4 attribute descriptions
    shaders/                 GLSL → SPIR-V (pre-compiled, include_bytes!)
  bsa/                       BSA archive reader (Bethesda Softworks Archive v104)
    src/archive.rs           BsaArchive: open, list, extract (zlib decompression)
  platform/                  Windowing (winit), raw handles
  nif/                       NIF file parser (Gamebryo .nif binary format)
    src/header.rs            NifHeader, version-aware header parsing
    src/version.rs           NifVersion (packed u32), version constants
    src/types.rs             NiPoint3, NiMatrix3, NiTransform, NiColor, BlockRef
    src/stream.rs            NifStream: version-aware binary reader
    src/blocks/              Block parsers: NiNode, NiTriShape/Strips, NiTriShapeData/StripsData, properties, BSShader, textures
    src/import.rs            NIF-to-ECS import: hierarchy preservation, geometry/transform conversion
    src/anim.rs              KF animation import: clips, channels, coordinate conversion
    src/scene.rs             NifScene: parsed block collection with downcasting
  ui/                       Scaleform/SWF UI (Ruffle integration)
    src/lib.rs               UiManager resource, SWF loading
    src/player.rs            SwfPlayer — Ruffle wrapper, offscreen wgpu rendering, pixel readback
  scripting/                 ECS-native scripting (events, timers)
    src/events.rs            Transient marker components: ActivateEvent, HitEvent, TimerExpired
    src/timer.rs             ScriptTimer component + timer_tick_system
    src/cleanup.rs           event_cleanup_system (end-of-frame marker removal)
  cxx-bridge/                C++ interop (cxx crate)
docs/
  engine/                    Engine documentation (architecture, ECS, renderer, etc.)
  legacy/                    Gamebryo 2.3 analysis (class hierarchy, NIF format, API)
```

## Architecture Invariants

1. **ECS over scene graph.** Components are data, systems are logic. No God Objects.
2. **Components declare their storage.** `PackedStorage` for hot-path (Transform), `SparseSetStorage` for sparse data (Name, MeshHandle).
3. **Interior mutability via RwLock.** Query/resource access takes `&self`. Structural mutation takes `&mut self`.
4. **TypeId-sorted lock acquisition** in multi-component queries to prevent deadlocks.
5. **Vulkan done properly.** Validation layers in debug, per-image semaphores, dynamic viewport/scissor, clean reverse-order teardown.
6. **No shortcuts on Vulkan init.** Full chain: entry → instance → debug → surface → physical device → logical device → allocator → swapchain → render pass → pipeline → framebuffers → command pool → sync.

## Critical Patterns

### ECS Component Declaration
```rust
struct MyComponent { ... }
impl Component for MyComponent {
    type Storage = SparseSetStorage<Self>;  // or PackedStorage<Self>
}
```

### System Signature
Systems take `&World` (not `&mut self`). Mutation via QueryWrite/ResourceWrite:
```rust
fn my_system(world: &World, dt: f32) {
    let mut q = world.query_mut::<Transform>().unwrap();
    for (entity, transform) in q.iter_mut() { ... }
}
```

### Multi-Component Query (intersection)
```rust
let (q_read, mut q_write) = world.query_2_mut::<Velocity, Position>().unwrap();
for (entity, vel) in q_read.iter() {
    if let Some(pos) = q_write.get_mut(entity) { ... }
}
```

## Legacy Engine Reference

Gamebryo 2.3 source: `/media/matias/Respaldo 2TB/Start-Game/Leaks/Gamebryo_2.3 SRC/Gamebryo_2.3/`

Key subsystems: `CoreLibs/NiMain/` (scene graph), `CoreLibs/NiAnimation/` (controllers/interpolators),
`CoreLibs/NiCollision/`, `CoreLibs/NiDX9Renderer/`, `CoreLibs/NiSystem/` (memory, threading, I/O).

NIF format: binary, 3-phase loading (parse → link → post-link). Version range 20.0.0.3–34.1.1.3.

Detailed analysis in `docs/legacy/`.

## Development Roadmap

See [ROADMAP.md](ROADMAP.md) for the full roadmap with milestones, known issues, and game compatibility.
Current: 22 milestones complete (M1–M22). RT multi-light with ray query shadows, animation with
blending stack, scene graph hierarchy, cell XCLL lighting, decal detection, BSA v103 (Oblivion).
Active: N23 series — NIF parser overhaul for Oblivion through Starfield (10 sub-milestones).
Usage:
  `cargo run -- path/to/mesh.nif` — render a loose NIF file
  `cargo run -- mesh.nif --kf anim.kf` — play animation on a mesh
  `cargo run -- --bsa path.bsa --mesh meshes\\foo.nif` — extract from BSA and render
  `cargo run -- --bsa meshes.bsa --mesh meshes\\foo.nif --textures-bsa textures.bsa` — with textures
  `cargo run -- --esm FalloutNV.esm --cell CellID --bsa Meshes.bsa --textures-bsa Textures.bsa` — cell
Done: N23.1 (trait hierarchy + FNV audit fixes — 7 parser bugs fixed).
Next: N23.2 (shader trailing fields), N23.4 (FNV validation), N23.3 (Oblivion).

## Git Conventions

- Conventional commit messages (what + why, not how)
- `Co-Authored-By: Claude Opus 4.6 (1M context) <noreply@anthropic.com>` on AI-assisted commits
- Branch: `main`
- Remote: `origin` → `github.com:matiaszanolli/ByroRedux.git`
