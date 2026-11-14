# Workspace map

The file-level map of the workspace, moved out of CLAUDE.md so it is read on
demand instead of loaded into every session. Confirm a path with `ls` / `rg`
before relying on it.

```
byroredux/              Binary — game loop, scene setup, systems
  src/main.rs              App struct + construction, main(), debug-UI snapshot bridge
  src/app_events.rs        ApplicationHandler — winit resumed/window/device/about_to_wait (#2731)
  src/app_frame.rs         App::render_one_frame — per-frame render driver (#2731)
  src/app_step.rs          Per-tick streaming / debug-load / save / cell-transition steppers
  src/components/          Marker components + app resources, split by domain
                           (render, terrain, environment, indices, input, audio,
                           tuning, ai_anim, navmesh, vats, explicit_start,
                           projectile_muzzle, game_time — #6ef8976f2's hub ~30 LOC)
  src/systems.rs           ECS systems: fly camera, animation, transform propagation, spin, stats
  src/scene.rs             Scene setup, NIF loading (load_nif_bytes, load_nif_from_args)
  src/asset_provider/      BSA/BA2-backed texture and mesh extraction
    mod.rs                   TextureProvider, resolve_texture, re-exports
    archive.rs               Archive — wraps BSA (Oblivion-Skyrim SE) or BA2 (FO4-Starfield)
    texture.rs                File-data lookup by searching BSA/BA2 archives
    material/                Material-path resolution incl. Starfield materialsbeta.cdb (provider, merge, cdb)
    script.rs                Compiled Papyrus (.pex) lookup by script name (M47.2 attach path)
    tests/                   Archive-provider regression tests
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
    assets.rs                Texture / mesh / skin diagnostics + tex.dump (archive → PNG)
    actor_value.rs           setav/modav — live-edit an actor's ActorValues
    condition.rs             cond — evaluate a CTDA condition function live
    world_info.rs            Engine / world / memory introspection commands
    view.rs                  Camera + selection/picking commands
    shared.rs                Cross-command formatting helpers + shared import prelude
    music.rs                 music.status — the music controller's layers (docs/engine/music.md)
  src/music/                Game music (docs/engine/music.md): per-game translate step + game-neutral controller
    mod.rs                   music_system, the audio-voice output, place tracking, music volume default
    directive.rs             Directive / outcomes, music-file resolution (folder forms), playlist shuffle
    translate.rs             Oblivion / FO3 / FNV sources → directives (cells, worldspaces, regions, DOBJ, audio markers)
    controller.rs            Hold / Sting / Script / Battle / Location layers, suspension, fades, music.status
    census_tests.rs          Ignored per-game source-coverage census (PRD scenario 1)
  src/game_ini.rs           The install's default INI: [section] key reads, plugin game detection
  src/helpers.rs            add_child, world_resource_set utilities
  src/hud.rs                MenuXml HUD driver — per-game profiles Oblivion/FO3/FNV (--hud, triple-buffered uploads)
  src/scaleform_hud.rs      Scaleform HUD driver — Skyrim + Fallout 4 hudmenu.swf via --hud (transparent stage, hud.debug)
  src/cell_loader.rs        ESM cell loading (interior + exterior)
  src/boot/                 CLI parsing, World/Scheduler construction, per-stage system registration (schedule/*)
  src/streaming/            M40 exterior cell streaming — live (gx,gy)→cell map, load/unload hysteresis
  src/save_io.rs            M45 binary-side save/load wiring — populates the SaveRegistry for byroredux-save
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
    src/esm/cell/            CELL walker + per-feature submodules (helpers / support / walkers / wrld,
                             plus the data tier: base_objects / cell_data / index / landscape /
                             lighting / placed_ref / worldspace)
      tests/                 CELL parsing regression tests (per-topic siblings)
  renderer/                  Vulkan graphics (ash, gpu-allocator, image)
    src/vulkan/              pipeline, device, swapchain, sync, allocator, buffer,
                             scene_buffer/ (SSBO/UBO), acceleration/ (BLAS/TLAS)
    src/vulkan/context/      VulkanContext
      mod.rs                 VulkanContext struct, new(), Drop (reverse-order teardown)
      draw.rs                draw_frame() — per-frame command recording + submission
      resize.rs              recreate_swapchain() — window resize handler
      resources.rs           build_blas_batched, register_ui_quad, swapchain_extent, log_memory_usage
      helpers.rs             find_depth_format, create_render_pass, create_framebuffers, etc.
    src/vulkan/acceleration/ AccelerationManager, BlasEntry, TlasState
      mod.rs                 Struct definition + new()/destroy(); scratch align rounds up at use (predicates::align_scratch_address)
      constants.rs           BLAS / TLAS slack margins, reserve floors, eviction thresholds
      types.rs               BlasEntry, TlasState data structs
      predicates.rs          Pure decision fns (`scratch_should_shrink`, `decide_use_update`, …)
      static_working_set.rs  Protects the current static-BLAS working set from LRU eviction
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
    src/vulkan/composite.rs  CompositePipeline — direct + denoised indirect + caustics + volumetrics reassembly, linear HDR out (#4202: tone map lives downstream in presentation)
    src/vulkan/presentation.rs Presentation pass — output-res exposure → IMGS/IMAD grade → chroma compress → tonemap (ACES|AgX), underwater extinction, swapchain write (#5482 order)
    src/vulkan/ssao.rs       SSAO compute pipeline (noise texture, kernel, screen-space AO)
    src/vulkan/descriptors.rs Descriptor set/pool management
    src/vulkan/compute.rs    Compute pipeline utilities
    src/vulkan/texture.rs    Texture upload (RGBA + BC-compressed DDS, staging, layout transitions)
    src/vulkan/dds.rs        DDS header parser (BC1/BC3/BC5, FourCC + DX10 extended, mip sizes)
    src/texture_registry/    TextureRegistry (path→handle cache, per-texture descriptor sets; lookup / upload / release)
    src/mesh.rs              MeshRegistry, global vertex/index SSBOs, cube/triangle/quad helpers
    src/vertex.rs            Vertex (position + color + normal + uv + bone_idx + bone_wt + splat0/1 + tangent), 9 attribute descriptions, 104 B (20 f32 + 4 u32 + 8 u8; color widened vec3→vec4, `cd2b5fe4`)
    shaders/                 GLSL → SPIR-V (pre-compiled, include_bytes!) — see crates/renderer/shaders/ for the full set; key passes:
      triangle.vert/frag     Main geometry pass — PBR + RT ray queries (shadows, reflections, GI)
      svgf_temporal.comp     SVGF temporal accumulation with motion vector reprojection
      taa.comp               TAA resolve (Halton jitter + YCoCg variance clamp, M37.5)
      composite.vert/frag    Fullscreen quad — direct + denoised indirect + caustics + volumetrics, linear HDR out (bloom + tone map live downstream)
      presentation.frag      Output-resolution presentation — exposure → grade → chroma compress → tonemap (ACES|AgX) + underwater extinction → swapchain
      ssao.comp              Screen-space ambient occlusion compute
      cluster_cull.comp      Clustered lighting frustum assignment
      skin_vertices.comp     GPU pre-skinning (M29)
      water.vert/frag        Water plane — vertex displacement + RT reflection/refraction (M38)
      caustic_splat.comp     Caustic splat compute (water under-side lighting)
      volumetrics_inject.comp / _integrate.comp  Volumetric froxel grid (M55)
      bloom_downsample.comp / _upsample.comp     Bloom pyramid (M58)
      bloom_apply.comp       Adds the bloom pyramid's top mip back into composite's HDR output, in place (after composite)
      exposure_meter.comp    Stage-1 auto-exposure meter — EV100 average + per-FIF adaptation, 1×1 exposure texel
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
        tangent.rs           Tangent extraction + Lengyel synthesis (nifly CalcTangentSpace)
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
  menuxml/                   Oblivion/FO3/FNV MenuXml UI (M48.4/M48.5 legacy-UI track)
    src/parse.rs             Tolerant XML scanner (vanilla quirks, <include> prefabs)
    src/eval.rs              Per-frame trait FOLD language + selectors + overrides
    src/layout.rs            Locus chains, depth sort, clipwindow scissor
    src/raster.rs            CPU source-over rasterizer (zoom: natural+clip default, -1 stretch)
    src/tex.rs               Own DDS decoder (BC1/2/3 + masked uncompressed) + .tex atlases
    src/font.rs              .fnt bitmap fonts
    src/menu.rs              MenuRenderer — sets, fonts, overrides, frames; runtime menu API (instantiate_template / graft_fragment)
    src/profile.rs           Per-game corpus profiles (font table, font archive, strings source)
    examples/render_hud.rs   Reference-frame + trait-probe visualizer
  mod-runtime/               WASM sandbox runtime (wasmtime 49, hostcall fuel #5718)
  pex/                       Compiled Papyrus (.pex) reader
  physics/                   Rapier physics world, water, footstep surface probe
  save/                      Save-file serialisation (shape-guarded)
  sdk/                       Script-extender SDK surface (script_function, hostcalls)
  settings-io/               profiles.toml + settings persistence
  sfmaterial/                Starfield .cdb material reader
  spt/                       SpeedTree (.spt) reader
  papyrus/                   Papyrus language parser (.psc source → AST)
    src/token.rs             Token enum (logos derive, case-insensitive keywords)
    src/lexer.rs             Lexer wrapper (line continuation, comments, doc comments)
    src/ast.rs               Full AST types (Script, Expr, Stmt, Type, all node kinds)
    src/span.rs              Span (byte offsets), Spanned<T> wrapper
    src/error.rs             ParseError with source-location diagnostics
    src/parser/              Recursive descent parser
      mod.rs                 Parser struct, token access, type parsing
      expr.rs                Pratt expression parser (precedence climbing)
  audio/                     Audio playback (descriptors, spatial tracks, one-shots)
  video/                     Movie decoding + the playback contract: Bink 1 via `mercs2_bink`, Bink 2 via bink2 + binkaudio
  bink2/                     Clean-room Bink 2 (KB2g/KB2j) video decoder + container — read its CLEANROOM.md first; spec in docs/spec/
  binkaudio/                 Clean-room Bink Audio decoder (Bink 2 soundtracks) — read its CLEANROOM.md first; spec in docs/spec/
  bgsm/                      Bethesda GS material (.bgsm/.bgem) reader
  boot-request/              Engine boot handshake types
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
    src/registration.rs      register_component::<T>() per inspectable type (re-derive the
                             count with `grep -c 'register_component::<' crates/debug-server/src/registration.rs`
                             rather than trusting a number here — it moves with every component added)
  debug-ui/                  Dear ImGui-style debug overlay types
  facegen/                   EGM/EGT FaceGen morph + texture readers
  fsr3-sys/                  FSR3 upscaler native bindings
  game-detect/               Installed-game discovery (registry / Steam library)
  hkx/                       Havok .hkx behavior-graph + clip reader
tools/
  byro-dbg/                  Standalone debug CLI binary
    src/main.rs              TCP client, REPL loop, shorthand commands
    src/display.rs           Pretty-print responses (entities, JSON, stats)
  byro-detect/               Game-install detection CLI (shares game-detect)
  byro-launcher/             Profile launcher CLI (shares the --profiles parser)
  nifskope/                  NIF inspection tooling (vendored; out of LOC scans)
  texture-upscale/           Offline texture upscaling tool
docs/
  engine/                    Engine documentation (architecture, ECS, renderer, etc.)
  legacy/                    Gamebryo 2.3 analysis (class hierarchy, NIF format, API)
```
