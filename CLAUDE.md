# ByroRedux

Clean rebuild of the Gamebryo/Creation engine lineage in Rust + C++ with Vulkan.
Long-term goal: load and render content from Gamebryo/Creation-era games.

## Quick Reference

```bash
cargo check                    # Type check (fast)
cargo test -p byroredux-core    # Run ECS/core tests (162 tests)
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
  src/main.rs              App struct, ApplicationHandler (winit event loop), main()
  src/components.rs        Marker components (Spinning, AlphaBlend, TwoSided, Decal) + app resources
  src/systems.rs           ECS systems: fly camera, animation, transform propagation, spin, stats
  src/scene.rs             Scene setup, NIF loading (load_nif_bytes, load_nif_from_args)
  src/asset_provider.rs    TextureProvider, BSA texture/mesh extraction, resolve_texture
  src/render.rs            Per-frame render data collection (build_render_data)
  src/anim_convert.rs      NIF→core animation clip conversion, subtree name map
  src/commands.rs           Console commands (help, stats, entities, systems)
  src/helpers.rs            add_child, world_resource_set utilities
  src/cell_loader.rs        ESM cell loading (interior + exterior)
crates/
  core/                      ECS, math (glam), types, string interning, form IDs
    src/ecs/                 World, Component, Storage, Query, System, Scheduler, Resource
    src/ecs/components/      Transform, GlobalTransform, Parent, Children, Camera, MeshHandle, Name,
                             FormIdComponent, LightSource, AnimatedVisibility/Alpha/Color
    src/ecs/resources.rs     DeltaTime, TotalTime, EngineConfig
    src/animation/           Animation engine (split into submodules)
      types.rs               CycleType, KeyType, key structs, channels, AnimationClip
      registry.rs            AnimationClipRegistry (Resource)
      player.rs              AnimationPlayer (Component), advance_time()
      stack.rs               AnimationLayer, AnimationStack, advance_stack(), sample_blended_transform()
      root_motion.rs         RootMotionDelta, split_root_motion()
      interpolation.rs       find_key_pair, hermite, TBC tangents, sample_translation/rotation/scale/float/color/bool
      text_events.rs         collect_text_key_events()
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
    src/vulkan/              pipeline, device, swapchain, sync, allocator, buffer,
                             scene_buffer (SSBO/UBO), acceleration (BLAS/TLAS)
    src/vulkan/context/      VulkanContext (split into submodules)
      mod.rs                 Struct definition (54 fields), new(), Drop (reverse-order teardown)
      draw.rs                draw_frame() — per-frame command recording + submission
      resize.rs              recreate_swapchain() — window resize handler
      resources.rs           build_blas_for_mesh, register_ui_quad, swapchain_extent, log_memory_usage
      helpers.rs             find_depth_format, create_render_pass, create_framebuffers, etc.
    src/vulkan/acceleration.rs  AccelerationManager, BlasEntry, TlasState (BLAS/TLAS lifecycle)
    src/vulkan/gbuffer.rs    GBuffer — normal, motion vector, mesh ID, raw indirect, albedo attachments
    src/vulkan/svgf.rs       SvgfPipeline — temporal accumulation denoiser for indirect lighting
    src/vulkan/composite.rs  CompositePipeline — direct + denoised indirect reassembly, ACES tone mapping
    src/vulkan/ssao.rs       SSAO compute pipeline (noise texture, kernel, screen-space AO)
    src/vulkan/descriptors.rs Descriptor set/pool management
    src/vulkan/compute.rs    Compute pipeline utilities
    src/vulkan/texture.rs    Texture upload (RGBA + BC-compressed DDS, staging, layout transitions)
    src/vulkan/dds.rs        DDS header parser (BC1/BC3/BC5, FourCC + DX10 extended, mip sizes)
    src/texture_registry.rs  TextureRegistry (path→handle cache, per-texture descriptor sets)
    src/mesh.rs              MeshRegistry, global vertex/index SSBOs, cube/triangle/quad helpers
    src/vertex.rs            Vertex (position + color + normal + UV), 4 attribute descriptions
    shaders/                 GLSL → SPIR-V (pre-compiled, include_bytes!)
      triangle.vert/frag     Main geometry pass — PBR + RT ray queries (shadows, reflections, GI)
      svgf_temporal.comp     SVGF temporal accumulation with motion vector reprojection
      composite.vert/frag    Fullscreen quad — direct + denoised indirect + ACES tone mapping
      ssao.comp              Screen-space ambient occlusion compute
      cluster_cull.comp      Clustered lighting frustum assignment
      ui.vert/frag           UI overlay (Scaleform/SWF)
  bsa/                       BSA + BA2 archive readers (Bethesda Softworks Archive)
    src/archive.rs           BsaArchive: BSA v103/v104/v105 (Oblivion → Skyrim SE)
    src/ba2.rs               Ba2Archive: BTDX v1/v2/v3/v7/v8 (FO4, FO76, Starfield),
                             GNRL + DX10 with reconstructed DDS headers
  platform/                  Windowing (winit), raw handles
  nif/                       NIF file parser (Gamebryo .nif binary format)
    src/header.rs            NifHeader, version-aware header parsing
    src/version.rs           NifVersion (packed u32), version constants
    src/types.rs             NiPoint3, NiMatrix3, NiTransform, NiColor, BlockRef
    src/stream.rs            NifStream: version-aware binary reader
    src/blocks/              Block parsers: NiNode, NiTriShape/Strips, NiTriShapeData/StripsData, properties, BSShader, textures
    src/import/              NIF-to-ECS import (split into submodules)
      mod.rs                 ImportedNode/Mesh/Scene types, import_nif_scene(), import_nif()
      walk.rs                Hierarchical + flat scene graph traversal
      mesh.rs                NiTriShape + BsTriShape geometry extraction
      material.rs            MaterialInfo, texture/alpha/decal property extraction
      transform.rs           Transform composition, degenerate rotation SVD repair
      coord.rs               Z-up (Gamebryo) → Y-up (renderer) quaternion conversion
    src/anim.rs              KF animation import: clips, channels, coordinate conversion
    src/scene.rs             NifScene: parsed block collection with downcasting
  ui/                       Scaleform/SWF UI (Ruffle integration)
    src/lib.rs               UiManager resource, SWF loading
    src/player.rs            SwfPlayer — Ruffle wrapper, offscreen wgpu rendering, pixel readback
  scripting/                 ECS-native scripting (events, timers)
    src/events.rs            Transient marker components: ActivateEvent, HitEvent, TimerExpired
    src/timer.rs             ScriptTimer component + timer_tick_system
    src/cleanup.rs           event_cleanup_system (end-of-frame marker removal)
  papyrus/                   Papyrus language parser (.psc source → AST)
    src/token.rs             Token enum (logos derive, case-insensitive keywords)
    src/lexer.rs             Lexer wrapper (line continuation, comments, doc comments)
    src/ast.rs               Full AST types (Script, Expr, Stmt, Type, all node kinds)
    src/span.rs              Span (byte offsets), Spanned<T> wrapper
    src/error.rs             ParseError with source-location diagnostics
    src/parser/              Recursive descent parser
      mod.rs                 Parser struct, token access, type parsing
      expr.rs                Pratt expression parser (precedence climbing)
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
Current: 23 milestones complete (M1–M22, M24 Phase 1, M26, M28 Phase 1) + N23 + N26 closeout
+ #178 end-to-end skinning (SkinnedMesh component → bone palette SSBO → unified shader path).
RT multi-light with ray query shadows, animation with blending stack, scene graph hierarchy,
cell XCLL lighting, decal detection, BSA v103 (Oblivion), 16× anisotropic filtering.
Active: N26 closeout — every "block silently dropped" issue closed. ~215 block types now parsed.
Usage:
  `cargo run -- path/to/mesh.nif` — render a loose NIF file
  `cargo run -- mesh.nif --kf anim.kf` — play animation on a mesh
  `cargo run -- --bsa path.bsa --mesh meshes\\foo.nif` — extract from BSA and render
  `cargo run -- --bsa meshes.bsa --mesh meshes\\foo.nif --textures-bsa textures.bsa` — with textures
  `cargo run -- --esm FalloutNV.esm --cell CellID --bsa Meshes.bsa --textures-bsa Textures.bsa` — cell
Done: N23.1–N23.10 all complete. 186 type names (156 parsed + 30 Havok skip).
Key: ~48 particle types, bhkCompressedMeshShape (Skyrim collision), FO4 half-float + shader
wetness, all 6 skinning blocks, full NiSkinPartition, NiPixelData, NiMorphData legacy keys.
Collision import with Havok→engine transform. Normal map from BSShaderPPLighting (FO3/FNV).
FO76/Starfield shader blocks: CRC32 flag arrays, Luminance/Translucency, stopcond on BGSM name.
Test infra: nif_stats example + per-game integration tests + graceful per-block parse recovery.
M26: BA2 reader (BTDX v1/v2/v3/v7/v8, GNRL + DX10) + NIF header BSStreamHeader fix for FO4/FO76.
M26+: Oblivion → 100% via header user_version threshold fix (10.0.1.0 → 10.0.1.8),
      BSStreamHeader for v10.0.1.2 / user_version>=3, and pre-Gamebryo empty-scene fallback.
Full-archive parse rates: ALL 7 games at 100% (177,286 NIFs). Oblivion was 99.13%.
M24 (Phase 1): records/ module with WEAP/ARMO/AMMO/MISC/KEYM/ALCH/INGR/BOOK/NOTE,
      CONT, LVLI/LVLN, NPC_, RACE, CLAS, FACT, GLOB, GMST. Real FNV.esm parses to
      13,684 structured records on top of cells in 0.19s release.
Session 6: Closed 26 GitHub issues. Critical fix: reverted #149's
NiTexturingProperty `Has Shader Textures: bool` gate (nif.xml was wrong;
Gamebryo 2.3 source reads u32 count directly). The bool-gate regression
was the root cause of "NiSourceTexture: failed to fill whole buffer"
spam on every Oblivion cell load — Anvil Heinrich Oaken Halls now
renders fully populated. Tools: new `crates/nif/examples/trace_block.rs`
that dumps per-block positions + 64-byte hex peeks for parser debugging.
Session 7: Starfield BA2 v3 DX10 texture extraction — v3 header has a
12-byte extension (not 8) with a compression_method field; LZ4 block
decompression via lz4_flex::block. Verified against 22 Starfield texture
archives (~128K DX10 textures) + 53 vanilla FO4 BA2s (v1/v7/v8), zero
failures. BA2 support now verified end-to-end for every version/variant.
Session 8: M30 Phase 1 — Papyrus language parser. New `byroredux-papyrus`
crate with logos lexer (case-insensitive keywords, line continuation,
comments) + Pratt expression parser (all operators, member access, indexing,
calls with named args, casts, new). Full AST types for the complete language.
45 tests. Phases 2–4 (statements, declarations, FO4 extensions) pending.
Next: M30 Phase 2 (statements + function bodies), M27 parallel scheduler,
M24 Phase 2 (QUST/DIAL/PERK), M28.5 kinematic character controller,
M29 GPU skinning compute path.

## Git Conventions

- Conventional commit messages (what + why, not how)
- `Co-Authored-By: Claude Opus 4.6 (1M context) <noreply@anthropic.com>` on AI-assisted commits
- Branch: `main`
- Remote: `origin` → `github.com:matiaszanolli/ByroRedux.git`
