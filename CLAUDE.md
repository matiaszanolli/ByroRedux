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

### Debug CLI
```bash
cargo run -p byro-dbg                       # Connect to running engine (port 9876)
BYRO_DEBUG_PORT=8080 cargo run -p byro-dbg  # Custom port
```

### Smoke tests
Manual end-to-end checks that need a Vulkan device + on-disk game data
(out of `cargo test` scope). All follow the same `--bench-hold` →
`byro-dbg`-attach pattern documented in
[`docs/smoke-tests/README.md`](docs/smoke-tests/README.md). Currently:
[`docs/smoke-tests/m41-equip.sh`](docs/smoke-tests/m41-equip.sh)
verifies Skyrim+ / FO4 NPC outfit equip end-to-end.

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
  src/asset_provider/      BSA/BA2-backed texture and mesh extraction
    mod.rs                   TextureProvider, resolve_texture, re-exports
    archive.rs               GameArchive — wraps BSA (Oblivion-Skyrim SE) or BA2 (FO4-Starfield)
    texture.rs                File-data lookup by searching BSA/BA2 archives
    material.rs              Material-path resolution incl. Starfield materialsbeta.cdb
    script.rs                Compiled Papyrus (.pex) lookup by script name (M47.2 attach path)
    tests.rs                  Archive-provider regression tests
  src/render/              Per-frame render data collection (build_render_data), split by pass
    mod.rs                   Top-level build_render_data + shared collection state
    camera.rs                View-projection + frustum setup
    lights.rs                Light collection
    particles.rs             Particle billboard emission
    sky.rs                   Sky parameter assembly
    skinned.rs               Skinned-mesh palette pass
    static_meshes.rs         Static mesh main loop
    water.rs                 Water-plane re-emit
    *_tests.rs               Per-pass regression tests (bone palette overflow, draw sort key, frustum, …)
  src/anim_convert.rs      NIF→core animation clip conversion, subtree name map
  src/commands/             Console commands (help, stats, entities, systems), split by topic
    mod.rs                   Command dispatch table
    scene.rs                 Scene / lighting / material / script-state commands
    assets.rs                Texture / mesh / skin diagnostic commands
    actor_value.rs           setav/modav — live-edit an actor's ActorValues
    condition.rs             cond — evaluate a CTDA condition function live
    world_info.rs            Engine / world / memory introspection commands
    view.rs                  Camera + selection/picking commands
    shared.rs                Cross-command formatting helpers + shared import prelude
  src/helpers.rs            add_child, world_resource_set utilities
  src/cell_loader.rs        ESM cell loading (interior + exterior)
crates/
  core/                      ECS, math (glam), types, string interning, form IDs
    src/ecs/                 World, Component, Storage, Query, System, Scheduler, Resource
    src/ecs/components/      Transform, GlobalTransform, Parent, Children, Camera, MeshHandle, Name,
                             FormIdComponent, LightSource, AnimatedVisibility/Alpha/Color
    src/ecs/resources/       DeltaTime, TotalTime, EngineConfig
      mod.rs                   Built-in engine resources
      skin_slot_pool.rs        Per-entity persistent bone-palette slot pool (bind_inverses SSBO, M29.6)
    src/animation/           Animation engine
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
    src/legacy/              Legacy ESM/ESP/ESL/ESH bridge (LegacyFormId, LegacyLoadOrder). Per-game parser stubs were removed under #390 — see `crates/plugin/src/esm/` for the live ESM path.
    src/esm/cell/            CELL walker + per-feature submodules (helpers / support / walkers / wrld)
      tests/                 CELL parsing regression tests (per-topic siblings)
  renderer/                  Vulkan graphics (ash, gpu-allocator, image)
    src/vulkan/              pipeline, device, swapchain, sync, allocator, buffer,
                             scene_buffer/ (SSBO/UBO), acceleration/ (BLAS/TLAS)
    src/vulkan/context/      VulkanContext
      mod.rs                 VulkanContext struct, new(), Drop (reverse-order teardown)
      draw.rs                draw_frame() — per-frame command recording + submission
      resize.rs              recreate_swapchain() — window resize handler
      resources.rs           build_blas_for_mesh, register_ui_quad, swapchain_extent, log_memory_usage
      helpers.rs             find_depth_format, create_render_pass, create_framebuffers, etc.
    src/vulkan/acceleration/ AccelerationManager, BlasEntry, TlasState
      mod.rs                 Struct definition + new()/destroy()/debug_assert_scratch_aligned()
      constants.rs           BLAS / TLAS slack margins, reserve floors, eviction thresholds
      types.rs               BlasEntry, TlasState data structs
      predicates.rs          Pure decision fns (`scratch_should_shrink`, `decide_use_update`, …)
      blas_static.rs         Static (mesh-keyed) BLAS lifecycle + builds + eviction
      blas_skinned.rs        Per-entity skinned BLAS lifecycle + refit
      tlas.rs                TLAS build / refit + `tlas_handle` accessor
      memory.rs              `shrink_*_to_fit` + telemetry getters
    src/vulkan/scene_buffer/ Per-frame scene SSBO/UBO
      mod.rs                 Re-exports + module docs
      constants.rs           MAX_INSTANCES, INSTANCE_FLAG_*, MATERIAL_KIND_* (every tunable)
      gpu_types.rs           `#[repr(C)]` shader-contract structs (GpuInstance, GpuLight, …)
      buffers.rs             `SceneBuffers` struct + `new()` + accessors + descriptor builder
      upload.rs              upload_lights / camera / bones / instances / materials / indirect / terrain
      descriptors.rs         write_ao_texture / geometry_buffers / cluster_buffers / tlas + destroy
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
    src/vertex.rs            Vertex (position + color + normal + uv + bone_idx + bone_wt + splat0/1 + tangent), 9 attribute descriptions, 100 B (19 f32 + 4 u32 + 8 u8)
    shaders/                 GLSL → SPIR-V (pre-compiled, include_bytes!) — see crates/renderer/shaders/ for the full set; key passes:
      triangle.vert/frag     Main geometry pass — PBR + RT ray queries (shadows, reflections, GI)
      svgf_temporal.comp     SVGF temporal accumulation with motion vector reprojection
      taa.comp               TAA resolve (Halton jitter + YCoCg variance clamp, M37.5)
      composite.vert/frag    Fullscreen quad — direct + denoised indirect + ACES tone mapping
      ssao.comp              Screen-space ambient occlusion compute
      cluster_cull.comp      Clustered lighting frustum assignment
      skin_vertices.comp     GPU pre-skinning (M29)
      water.vert/frag        Water plane — vertex displacement + RT reflection/refraction (M38)
      caustic_splat.comp     Caustic splat compute (water under-side lighting)
      volumetrics_inject.comp / _integrate.comp  Volumetric froxel grid (M55)
      bloom_downsample.comp / _upsample.comp     Bloom pyramid (M58)
      ui.vert/frag           UI overlay (Scaleform/SWF)
  bsa/                       BSA + BA2 archive readers (Bethesda Softworks Archive)
    src/archive/             BsaArchive: BSA v103/v104/v105 (Oblivion → Skyrim SE)
      mod.rs                   Module docs + BsaArchive struct
      open.rs                  Header + folder/file record table walk
      extract.rs               Per-file extraction (zlib v103/v104, LZ4 frame v105)
      hash.rs                  Folder/file name hash functions (debug/test-only)
      tests.rs                 Integration + synthetic-fixture tests
    src/ba2.rs               Ba2Archive: BTDX v1/v2/v3/v7/v8 (FO4, FO76, Starfield),
                             GNRL + DX10 with reconstructed DDS headers
  platform/                  Windowing (winit), raw handles
  nif/                       NIF file parser (Gamebryo .nif binary format)
    src/header.rs            NifHeader, version-aware header parsing
    src/version.rs           NifVersion (packed u32), version constants
    src/types.rs             NiPoint3, NiMatrix3, NiTransform, NiColor, BlockRef
    src/stream.rs            NifStream: version-aware binary reader
    src/blocks/              Block parsers: NiNode, NiTriShape/Strips, properties, BSShader, textures, …
      collision/             bhk* parsers
        mod.rs               Re-exports + shared low-level readers (`read_havok_material`, `read_vec4`, `read_matrix3`)
        collision_object.rs  Base + Bhk + BhkNP + BhkP + SystemBinary
        rigid_body.rs        `BhkRigidBody`
        ragdoll.rs           bone-pose + ragdoll templates (FO3+)
        shape_primitive.rs   Sphere / MultiSphere / Box / Capsule / Cylinder
        shape_compound.rs    Convex / List / Transform / MoppBvTree / ConvexList
        shape_mesh.rs        NiTriStrips / PackedNiTriStrips + per-strip data
        compressed_mesh.rs   Skyrim+ `BhkCompressedMeshShape` + data
        constraints.rs       `BhkConstraint`, `BhkBreakableConstraint`
        phantom_action.rs    Phantoms + `LiquidAction` + `OrientHingedBodyAction`
      dispatch_tests/        Block-dispatch regression tests
    src/import/              NIF-to-ECS import
      mod.rs                 ImportedNode/Mesh/Scene types, import_nif_scene(), import_nif()
      walk/                  Hierarchical + flat scene graph traversal
        mod.rs                 walk_node_hierarchical, walk_node_flat, satellite walkers (lights, particle emitters, …)
        tests.rs               Traversal regression tests
      mesh/                  Geometry extraction (production + test siblings)
        mod.rs               Re-exports + module docs
        material_path.rs     `material_path_from_name` (`.bgsm`/`.bgem` capture)
        decode.rs            half-float / byte-normal / LE readers
        ni_tri_shape.rs      Classic `NiTriShape` + `GeomData<'a>`
        bs_tri_shape.rs      Skyrim SE+ packed-half BSTriShape
        bs_geometry.rs       Starfield `BSGeometry` extraction
        tangent.rs           Tangent extraction + Mikkelsen synthesis
        sse_recon.rs         Skyrim SE skinned-geometry reconstruction (#559)
        skin.rs              Skinning data extraction + bone-pose flattening
      material/              MaterialInfo, texture/alpha/decal property extraction
        mod.rs                 Re-exports + module docs
        walker.rs              Shader-property tree walker
        shader_data.rs         Shader-type data extraction
        *_tests.rs             Per-behavior regression tests (alpha flag, emissive source, FO4 shader flags, PBR translation, …)
      transform.rs           Transform composition, degenerate rotation SVD repair
      coord.rs               Z-up (Gamebryo) → Y-up (renderer) quaternion conversion
    src/anim/                KF animation import
      mod.rs                 Re-exports + module docs
      coord.rs               Zup → Yup helpers
      controlled_block.rs    `CbString` + string / target resolution
      transform.rs           TRS channel extraction
      sequence.rs            Per-`NiControllerSequence` import
      keys.rs                Key conversion + Euler ↔ quat
      channel.rs             Float / Color / Bool / texture-transform channels
      bspline.rs             Compressed B-spline evaluation (#155)
      entry.rs               `import_kf` + `import_embedded_animations` public entry points
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
  debug-protocol/            Wire types + component registry for debug CLI
    src/lib.rs               DebugRequest, DebugResponse, EntityInfo
    src/wire.rs              Length-prefixed JSON encode/decode
    src/registry.rs          ComponentDescriptor, ComponentRegistry (type-erased accessors)
  debug-server/              TCP debug server embedded in engine
    src/lib.rs               start() entry point, SystemList re-export
    src/listener.rs          TcpListener, per-client threads, command queue
    src/system.rs            DebugDrainSystem (Late-stage exclusive), screenshot flow
    src/evaluator.rs         Papyrus AST → ECS query evaluation
    src/registration.rs      register_component::<T>() for 15 inspectable types
tools/
  byro-dbg/                  Standalone debug CLI binary
    src/main.rs              TCP client, REPL loop, shorthand commands
    src/display.rs           Pretty-print responses (entities, JSON, stats)
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

## Current State & Roadmap

30+ milestones complete: RT multi-light (streaming RIS), BLAS compaction + LRU
eviction, instanced draw batching, landscape terrain + splatting, exterior sun,
TAA, SVGF, Papyrus parser, GPU skinning, water + volumetrics + bloom. NIF parser
spans Oblivion → Starfield; the live block-dispatcher arm count lives in
`crates/nif/src/blocks/mod.rs`. Archives: BSA v103–v105 + BA2 v1–v8.

**Authoritative sources — do not duplicate state into this file:**
- [ROADMAP.md](ROADMAP.md) — milestones, known issues, per-game **compat matrix +
  parse rates**, project stats (test/file/LOC counts). Refreshed each `/session-close`.
- [HISTORY.md](HISTORY.md) — session-by-session narratives + audit closeouts.
- `git log` — fine-grained archaeology.

## Usage

```bash
cargo run -- path/to/mesh.nif                       # render a loose NIF
cargo run -- mesh.nif --kf anim.kf                  # mesh + animation
cargo run -- --esm FalloutNV.esm --cell <id> --bsa "Fallout - Meshes.bsa" --textures-bsa "Fallout - Textures.bsa"   # interior cell (vanilla FNV archive names; `--bsa` opens the literal path — there is no bare Meshes.bsa)
cargo run -- --esm FalloutNV.esm --grid 0,0 --radius 3 --bsa …                              # exterior grid (1..=7)
cargo run -- --master Skyrim.esm --esm Dawnguard.esm --cell <id> --bsa …                    # DLC interior (repeatable --master)
cargo run --release -- … --bench-frames 300 --bench-hold                                    # bench, then HOLD open for byro-dbg
```
Operational gotchas worth knowing up front:
- `<stem>N.bsa` siblings auto-load (`Textures.bsa` drags in `Textures2.bsa`) — see `asset_provider/archive.rs`.
- `--bench-hold` keeps the engine alive so `byro-dbg` can attach (port 9876) and run
  console commands (`tex.missing`, `tex.loaded`, …); without it the bench exits and the
  debug server is unreachable.
- "Chrome / posterized" surfaces usually mean **missing textures** (checker placeholder ×
  normal map), not a lighting bug — run `tex.missing` first.

Full invocation set in [README.md](README.md#run).

## Git Conventions

- Conventional commit messages (what + why, not how)
- No `Co-Authored-By` / AI co-author trailer — commit body only (per global instruction)
- Branch: `main` · Remote: `origin` → `github.com:matiaszanolli/ByroRedux.git`
