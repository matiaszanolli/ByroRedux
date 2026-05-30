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
    src/legacy/              Legacy ESM/ESP/ESL/ESH bridge (LegacyFormId, LegacyLoadOrder). Per-game parser stubs were removed under #390 — see `crates/plugin/src/esm/` for the live ESM path.
    src/esm/cell/            CELL walker + per-feature submodules (helpers / support / walkers / wrld)
      tests/                 CELL parsing regression tests (Session 35 split — 8 per-topic siblings: light, addn_stat, refr, cell, txst, merge, wrld, integration)
  renderer/                  Vulkan graphics (ash, gpu-allocator, image)
    src/vulkan/              pipeline, device, swapchain, sync, allocator, buffer,
                             scene_buffer/ (SSBO/UBO), acceleration/ (BLAS/TLAS)
    src/vulkan/context/      VulkanContext (split into submodules)
      mod.rs                 VulkanContext struct, new(), Drop (reverse-order teardown)
      draw.rs                draw_frame() — per-frame command recording + submission
      resize.rs              recreate_swapchain() — window resize handler
      resources.rs           build_blas_for_mesh, register_ui_quad, swapchain_extent, log_memory_usage
      helpers.rs             find_depth_format, create_render_pass, create_framebuffers, etc.
    src/vulkan/acceleration/ AccelerationManager, BlasEntry, TlasState (Session 35 split)
      mod.rs                 Struct definition + new()/destroy()/debug_assert_scratch_aligned()
      constants.rs           BLAS / TLAS slack margins, reserve floors, eviction thresholds
      types.rs               BlasEntry, TlasState data structs
      predicates.rs          Pure decision fns (`scratch_should_shrink`, `decide_use_update`, …)
      blas_static.rs         Static (mesh-keyed) BLAS lifecycle + builds + eviction
      blas_skinned.rs        Per-entity skinned BLAS lifecycle + refit
      tlas.rs                TLAS build / refit + `tlas_handle` accessor
      memory.rs              `shrink_*_to_fit` + telemetry getters
    src/vulkan/scene_buffer/ Per-frame scene SSBO/UBO (Session 35 split)
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
    src/archive.rs           BsaArchive: BSA v103/v104/v105 (Oblivion → Skyrim SE)
    src/ba2.rs               Ba2Archive: BTDX v1/v2/v3/v7/v8 (FO4, FO76, Starfield),
                             GNRL + DX10 with reconstructed DDS headers
  platform/                  Windowing (winit), raw handles
  nif/                       NIF file parser (Gamebryo .nif binary format)
    src/header.rs            NifHeader, version-aware header parsing
    src/version.rs           NifVersion (packed u32), version constants
    src/types.rs             NiPoint3, NiMatrix3, NiTransform, NiColor, BlockRef
    src/stream.rs            NifStream: version-aware binary reader
    src/blocks/              Block parsers: NiNode, NiTriShape/Strips, properties, BSShader, textures, …
      collision/             bhk* parsers (Session 35 split — 9 siblings)
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
      dispatch_tests/        Block-dispatch regression tests (Session 35 split — 9 per-topic siblings)
    src/import/              NIF-to-ECS import (split into submodules)
      mod.rs                 ImportedNode/Mesh/Scene types, import_nif_scene(), import_nif()
      walk.rs                Hierarchical + flat scene graph traversal
      mesh/                  Geometry extraction (Session 35 split — 8 production + 7 test siblings)
        mod.rs               Re-exports + module docs
        material_path.rs     `material_path_from_name` (`.bgsm`/`.bgem` capture)
        decode.rs            half-float / byte-normal / LE readers
        ni_tri_shape.rs      Classic `NiTriShape` + `GeomData<'a>`
        bs_tri_shape.rs      Skyrim SE+ packed-half BSTriShape
        bs_geometry.rs       Starfield `BSGeometry` extraction
        tangent.rs           Tangent extraction + Mikkelsen synthesis
        sse_recon.rs         Skyrim SE skinned-geometry reconstruction (#559)
        skin.rs              Skinning data extraction + bone-pose flattening
      material.rs            MaterialInfo, texture/alpha/decal property extraction
      transform.rs           Transform composition, degenerate rotation SVD repair
      coord.rs               Z-up (Gamebryo) → Y-up (renderer) quaternion conversion
    src/anim/                KF animation import (Session 35 split — 8 per-phase siblings)
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

## Development Roadmap

See [ROADMAP.md](ROADMAP.md) for the full roadmap with milestones, known issues, and game compatibility.
Current: 30+ milestones complete (M1–M22, M24 Phase 1, M26, M28 Phase 1, M30 Phase 1, M31,
M31.5, M32 Phase 1+2, M34 Phase 1, M36, M37.5) + N23 + N26 closeout + #178 skinning.
RT multi-light with streaming RIS (16 reservoirs/fragment, Phase 19), BLAS compaction + LRU eviction,
instanced draw batching, landscape terrain with texture splatting, exterior directional sun,
TAA (Halton jitter + YCoCg variance clamp), Papyrus language parser, FO4 SCOL/MOVS/PKIN/TXST.
See [ROADMAP.md](ROADMAP.md#project-stats) for ground truth on test/file/LOC/crate counts (refreshed each /session-close).
Usage:
  `cargo run -- path/to/mesh.nif` — render a loose NIF file
  `cargo run -- mesh.nif --kf anim.kf` — play animation on a mesh
  `cargo run -- --bsa path.bsa --mesh meshes\\foo.nif` — extract from BSA and render
  `cargo run -- --bsa meshes.bsa --mesh meshes\\foo.nif --textures-bsa textures.bsa` — with textures
  `cargo run -- --bsa "Fallout - Meshes.bsa" --tree trees\\joshua01.spt --textures-bsa "Fallout - Textures.bsa"` — direct SpeedTree visualiser (FNV/FO3/Oblivion `.spt`); renders a placeholder billboard per SpeedTree Phase 1.6
  `cargo run -- --esm FalloutNV.esm --cell CellID --bsa Meshes.bsa --textures-bsa Textures.bsa` — cell
  `cargo run -- --esm Fallout3.esm --cell Megaton01 --bsa "Fallout - Meshes.bsa" --textures-bsa "Fallout - Textures.bsa"` — FO3 interior cell (Megaton, 929 REFRs)
  `cargo run -- --esm FalloutNV.esm --grid 0,0 --radius 3 --bsa …` — exterior grid (radius 1..=7, default 3)
  `cargo run -- --master Skyrim.esm --esm Dawnguard.esm --cell ForebearsHoldoutInt01 --bsa …` — DLC interior (M46.0 / #561, repeatable `--master`)
  `cargo run --release -- … --bench-frames 300 --bench-hold` — run 300-frame bench, print summary, **keep the engine open** so `byro-dbg` can attach (port 9876) and drive console commands against the loaded scene. Without `--bench-hold` the bench exits immediately and the debug server isn't reachable for `tex.missing` / `tex.loaded` / etc.
Done: N23.1–N23.10 all complete. NIF block dispatcher in `crates/nif/src/blocks/mod.rs` carries the live arm count — see source.
Key: ~48 particle types, bhkCompressedMeshShape (Skyrim collision), FO4 half-float + shader
wetness, all 6 skinning blocks, full NiSkinPartition, NiPixelData, NiMorphData legacy keys.
Collision import with Havok→engine transform. Normal map from BSShaderPPLighting (FO3/FNV).
FO76/Starfield shader blocks: CRC32 flag arrays, Luminance/Translucency, stopcond on BGSM name.
Test infra: nif_stats example + per-game integration tests + graceful per-block parse recovery.
M26: BA2 reader (BTDX v1/v2/v3/v7/v8, GNRL + DX10) + NIF header BSStreamHeader fix for FO4/FO76.
M26+: Oblivion clean-parse fixes via header user_version threshold (10.0.1.0 → 10.0.1.8),
      BSStreamHeader for v10.0.1.2 / user_version>=3, and pre-Gamebryo empty-scene fallback.
Full-archive parse rates — **ROADMAP.md compat matrix is the authoritative
source** (refreshed each `/session-close`); the snapshot below is informational
only and is allowed to drift one sweep behind. Latest sweep 2026-04-27:
clean=100% on FO3 / FNV / Skyrim SE; Oblivion 96.24%, FO4 96.46%,
FO76 97.34%, Starfield 98.6% aggregate (drift-induced truncation
tracked at #687 / #688 / #697 / #698; SF jumped from 0.80% after #708
closeout — BSGeometry / SkinAttach / BoneTranslations now dispatch;
further Starfield gain post-#754 BSWeakReferenceNode). Recoverable
rate is 100% on all except Oblivion's single hard-fail on a corrupt-
by-design debug marker (#698). Per-archive NIF counts in
[ROADMAP.md](ROADMAP.md) compat matrix.
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
decompression via lz4_flex::block. Verified against the 30 Starfield
texture archives shipped post-Shattered-Space (was 22 as of Session 7;
re-checked 2026-05-21, #1185) + 53 vanilla FO4 BA2s (v1/v7/v8), zero
failures. BA2 support now verified end-to-end for every version/variant.
Session 8: 35 commits. M30 Phase 1 — Papyrus language parser (logos lexer +
Pratt expression parser, 45 tests). M31 — RT performance at scale (batched
BLAS builds, TLAS culling, importance-sorted shadow budget, distance-based
ray fallback, GI hit simplification, BLAS LRU eviction, deferred SSBO
rebuild). M32 Phase 1+2 — landscape terrain from LAND heightmap records
with LTEX/TXST texture splatting. M34 Phase 1 — default exterior sun for
directional lighting. Fix #251–#284: alpha test function extraction (#263),
dark texture import (#264), instanced draw batching (#272), shadow ray
budget (#270), subtree cache persistence (#278), Vulkan sync fixes (#280–
#284), NIF string read optimization (#254), animation scratch buffers
(#251–#252), performance bundle (#279). Roadmap reprioritized to renderer-
first with M32–M48 tiered plan.
Session 11: 72-commit bug-bash on the #341–#438 audit bundle.
Parser correctness (Oblivion v20.0.0.5 stability — runtime size cache,
stream drift detector, v20.2.0.5+ parallax gate). Import path correctness
(normal-map routing, NiDynamicEffect affected_nodes, material_kind,
BSDynamicTriShape vertex extraction, all-8 TXST slots, VMAD has_script).
NIF import cache promoted to process-lifetime resource (#381).
Sync/cache hardening: VkPipelineCache plumbed through every create site,
per-(src, dst, two_sided) blend pipeline cache, TLAS build barrier
widened, TRIANGLE_FACING_CULL_DISABLE gated on two_sided,
gl_RayFlagsTerminateOnFirstHitEXT on reflection + glass rays.
Session 12 (2026-04-19/20): AUDIT_FO3 + AUDIT_FNV + AUDIT_ECS sweep.
Parser correctness:
  — #408 blanket `allocate_vec` sweep (60+ sites across 12 NIF files);
  — #440 `BSGeometryDataFlags` vs `NiGeometryDataFlags` split — FO3 FaceGen
    heads render geometry correctly (was NiUnknown-demoted);
  — #402 Oblivion KF deprecated `Ref<NiStringPalette>` trailer +
    palette-backed string resolution — `NiTransformData` parsed: 3 → 40,623;
  — #455 `TileShaderProperty` dedicated parser (was aliased to PPLighting);
  — #333 `matrix3_to_quat` fast-path normalisation;
  — #441 removed bogus SF_DOUBLE_SIDED on FO3/FNV (that bit is `Unknown_3`);
  — #454 shared decal-flag helper so NoLighting/PPLighting stay in lockstep;
  — #329 / #330 NiExtraData version gating (pre-10.0.1.0 `Name` absent);
  — #350 BSShaderController tagged kind — animation importer routes.
ESM dispatch expansion (10 → 18 record categories):
  — #442 CREA (533 in FO3), #448 LVLC (60), #443 SCPT pre-Papyrus bytecode
    (1257, 1184 with SCRV/SCRO cross-refs), #458 WATR/NAVI/NAVM/REGN/
    ECZN/LGTM/HDPT/EYES/HAIR stubs.
Renderer plumbing:
  — #452 / #453 BSShaderTextureSet slots 3/4/5 → GpuInstance with POM
    fragment branch (struct size pinned by `gpu_instance_is_112_bytes_std430_compatible`
    test; Shader Struct Sync lockstep across the shaders that declare
    `struct GpuInstance` — see `feedback_shader_struct_sync.md`);
  — #421 window portal ray fires along -N with grazing-angle gate;
  — #464 BFS transform propagation via VecDeque.
Compat correctness:
  — #439 HEDR→GameKind bands verified against disk-sampled masters;
  — #445 `FormIdRemap` + `parse_esm_with_load_order` for multi-plugin
    collision-free loads (CLI stays single-plugin today);
  — #444 worldspace auto-pick adds FO3 `wasteland` EDID + `--wrld`
    override + grid-containing preference;
  — #463 CLMT TNAM hours → `weather_system` per-worldspace TOD clock.
Docs hygiene:
  — #456 date-stamped stale FPS claims across ROADMAP + game-compat;
  — #457 FO3 Tier-1 row updated to "Interior ✓ · Exterior wired".
  — Megaton validated parse-side at 929 REFRs (was 1609 post-NIF-expand).
See [ROADMAP.md](ROADMAP.md#project-stats) for current test count and per-game compat matrix.
Next: M29.5 GPU palette dispatch, M35 terrain LOD, M37 SVGF spatial filter,
M37.3 ReSTIR-DI, FO3 exterior GPU re-bench (#457). M33 (sky/atmosphere) and
M29 (GPU skinning) closed — confirm via ROADMAP before treating as upcoming.
Session 27 (2026-05-02): "Chrome posterized walls" red herring nailed
to the wrong cause across multiple sessions. Auto-loaded
`<stem>N.bsa` siblings in `byroredux/src/asset_provider.rs` —
`Fallout - Textures.bsa` now drags in `Fallout - Textures2.bsa`
without a second `--textures-bsa` flag. Diagnosis: `tex.missing`
reported 39 unique missing textures × 263 entities for
`GSDocMitchellHouse` (walls + floor + trim); the chrome look was
the magenta-checker placeholder × the (correctly loaded) tangent-
space normal map. Added `DBG_BYPASS_NORMAL_MAP = 0x10` as a
permanent bisect bit alongside `DBG_VIZ_NORMALS` / `DBG_VIZ_TANGENT`.
With both texture archives loaded `tex.missing` drops to 1
(`<no path, no material>` placeholder geometry). New feedback memory:
when an artifact reads as "chrome / posterized," run `tex.missing`
before suspecting lighting code.

## Git Conventions

- Conventional commit messages (what + why, not how)
- `Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>` on AI-assisted commits
- Branch: `main`
- Remote: `origin` → `github.com:matiaszanolli/ByroRedux.git`
