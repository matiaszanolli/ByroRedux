# Shared Audit Protocol — ByroRedux

This file is referenced by all audit skills. Do NOT use as a slash command (prefixed with `_`).

## Project Layout

```
Core ECS:        crates/core/src/ecs/
Components:      crates/core/src/ecs/components/
Animation:       crates/core/src/animation/          (types, player, stack, registry, interpolation, root_motion, text_events, controller)
Resources:       crates/core/src/ecs/resources/       (mod.rs + skin_slot_pool.rs, split under #1869)
Strings:         crates/core/src/string/
NIF Parser:      crates/nif/src/
NIF Blocks:      crates/nif/src/blocks/               (see blocks/mod.rs dispatch; controller/ subdir, tri_shape/ subdir {mod, ni_tri_shape, bs_tri_shape, agd}, collision/ subdir {mod, collision_object, rigid_body, ragdoll, shape_primitive, shape_compound, shape_mesh, compressed_mesh, constraints, phantom_action}, particle.rs (typed NiPSysEmitter/NiPSysEmitterCtlr/NiPSysEmitterCtlrData/NiPSysGrowFadeModifier), shader.rs, skin.rs, properties.rs, interpolator.rs, extra_data.rs, light.rs, multibound.rs, palette.rs, legacy_particle.rs, texture.rs, bs_geometry.rs, node.rs, base.rs, traits.rs; *_tests.rs siblings)
NIF Import:      crates/nif/src/import/               (mod.rs thin dispatch + types.rs + tests.rs; walk/{mod, tests} (mod.rs carries extract_emitter_params/extract_emitter_rate), mesh/{mod, material_path, decode, ni_tri_shape, bs_tri_shape, bs_geometry, tangent, sse_recon, skin, *_tests}, material/{mod, walker, shader_data, *_tests}, transform.rs, coord.rs, collision.rs (translates BhkMultiSphereShape + BhkConvexListShape → CollisionShape), precombine.rs (M49 FO4 precombined PSG slice → renderer-space mesh, paired with CsgArchive))
NIF Animation:   crates/nif/src/anim/                 (mod.rs re-exports; coord, controlled_block, transform, sequence, keys, channel, bspline, entry; types.rs + tests.rs)
BSA Reader:      crates/bsa/src/archive/             (mod.rs, open.rs, extract.rs, hash.rs, tests.rs)
BA2 Reader:      crates/bsa/src/ba2.rs
CSG Reader:      crates/bsa/src/csg.rs               (FO4 precombined geometry; BSPackedGeomObject TLV; M49. Spec: docs/engine/fo4-csg-format.md. Consumed by cell_loader/precombined.rs)
BGSM Materials:  crates/bgsm/src/                     (FO4+ external material parser)
SF Material:     crates/sfmaterial/src/               (Starfield CDB material consumer: chunk, reader, string_table, types, value)
FaceGen (M41):   crates/facegen/src/                  (.tri/.egt morph + texture blend)
Physics (M28):   crates/physics/src/                  (Rapier3D bridge)
Papyrus (M30):   crates/papyrus/src/                  (.psc lexer + Pratt parser → AST: token, lexer, ast, span, error, parser/{mod, expr})
Pex (M47.2):     crates/pex/src/                      (compiled-Papyrus .pex → AST decompiler, Champollion port: opcode, reader, model, decompile/{mod, cfg, lift, control_flow, lower, boolean, node, event_names}. 5-phase: CFG → node-lift+copy-prop → control-flow recon → AST lower+fidelity gate → short-circuit booleans)
Scripting (M12/M47): crates/scripting/src/            (ECS-native scripting runtime: events, timer, cleanup, condition (M47.1 cond eval), trigger (M47.2 TriggerVolume detection), quest_stages, fragment, recurring_update, registry; translate/ holds the AST→ECS recognizer chain {mod, source, archetype, compose, effects, tables, recognizers/{mod, quest_stage_gate, rumble}}; papyrus_demo/ holds hand-verified reference scripts)
Save (M45):      crates/save/src/                     (full-ECS-snapshot save/load: snapshot, registry, disk, validate, driver; M45.1 live load-apply = reload cell + FormId-keyed deltas + player-pose restore)
Audio (M44):     crates/audio/src/lib.rs + tests.rs   (byroredux-audio: kira backend, AudioWorld resource, AudioListener/AudioEmitter/OneShotSound components, audio_system, SoundCache, streaming music, global reverb send)
SpeedTree (S1):  crates/spt/src/                      (byroredux-spt: TLV walker for FNV/FO3/Oblivion .spt; placeholder-billboard import fallback)
Debug Protocol:  crates/debug-protocol/src/           (wire types, component registry)
Debug Server:    crates/debug-server/src/             (TCP server + DebugDrainSystem)
Debug UI (egui): crates/debug-ui/src/                 (lib.rs, panels.rs — egui overlay)
Renderer:        crates/renderer/src/vulkan/
VulkanContext:   crates/renderer/src/vulkan/context/  (mod.rs, draw.rs, resize.rs, resources.rs, helpers.rs, screenshot.rs)
Accel (RT):      crates/renderer/src/vulkan/acceleration/  (mod.rs struct + new()/destroy(); constants, types, predicates, blas_static, blas_skinned, tlas, memory; tests.rs)
G-Buffer:        crates/renderer/src/vulkan/gbuffer.rs
SVGF Denoiser:   crates/renderer/src/vulkan/svgf.rs
TAA (M37.5):     crates/renderer/src/vulkan/taa.rs
Composite:       crates/renderer/src/vulkan/composite.rs
SSAO:            crates/renderer/src/vulkan/ssao.rs
Caustics (M22):  crates/renderer/src/vulkan/caustic.rs       (#321 Option A: per-frame compute splat into R32_UINT accumulator)
Water Caustic:   crates/renderer/src/vulkan/water_caustic.rs (#1210/#1255 Phase C: per-FIF R32_UINT accumulator for water-side caustics)
GPU Timers:      crates/renderer/src/vulkan/gpu_timers.rs
egui Pass:       crates/renderer/src/vulkan/egui_pass.rs      (egui overlay render pass; feeds debug-ui)
Volumetrics(M55):crates/renderer/src/vulkan/volumetrics.rs  (160×90×128 froxel grid, inject + integrate compute, single-ray TLAS shadow, HG phase)
Bloom (M58):     crates/renderer/src/vulkan/bloom.rs        (5-mip down + 4-mip up pyramid, B10G11R11_UFLOAT, 4-tap bilinear)
Water (M38):     crates/renderer/src/vulkan/water.rs        (WaterPipeline: vertex displacement + Fresnel, RT reflection/refraction against TLAS)
GPU Skin (M29):  crates/renderer/src/vulkan/skin_compute.rs
Material (R1):   crates/renderer/src/vulkan/material.rs   (MaterialBuffer SSBO, GpuMaterial dedup; replaces per-instance fields)
SPIR-V Reflect:  crates/renderer/src/vulkan/reflect.rs    (descriptor layout reflection from SPIR-V)
Scene Buffers:   crates/renderer/src/vulkan/scene_buffer/  (mod, constants, gpu_types, buffers, upload, descriptors; gpu_instance_layout_tests + instance_hash_tests + material_hash_tests + scene_descriptor_reflection_tests)
Descriptors:     crates/renderer/src/vulkan/descriptors.rs
Vk Debug Util:   crates/renderer/src/vulkan/debug.rs
Vk Instance:     crates/renderer/src/vulkan/instance.rs
Vk Surface:      crates/renderer/src/vulkan/surface.rs
Mesh:            crates/renderer/src/mesh.rs
Vertex:          crates/renderer/src/vertex.rs
Tex Registry:    crates/renderer/src/texture_registry.rs (+ texture_registry_tests.rs)
Shaders:         crates/renderer/shaders/             (triangle.vert/frag, svgf_temporal.comp, taa.comp, composite.vert/frag, ssao.comp, cluster_cull.comp, skin_palette.comp, skin_vertices.comp, caustic_splat.comp, volumetrics_inject.comp, volumetrics_integrate.comp, bloom_downsample.comp, bloom_upsample.comp, water.vert/frag, ui.vert/frag — full per-pass roles and G-buffer layout in docs/engine/shader-pipeline.md)
Plugin/ESM:      crates/plugin/src/                   (esm/{mod, reader, sub_reader}, esm/cell/, esm/records/{actor, climate, container, global, items, misc/{water, character, world, ai, magic, effects, equipment}, mswp, pkin, scol, script, tree, weather, …}, record.rs generic dispatch; legacy/ holds the LegacyFormId/LoadOrder bridge — per-game stubs were removed under #390)
Platform:        crates/platform/src/
UI (Ruffle):     crates/ui/src/
CXX Bridge:      crates/cxx-bridge/
Binary:          byroredux/src/main.rs
Systems:         byroredux/src/systems.rs (module index) → systems/{animation, audio, billboard, bounds, camera, character, debug, light_anim, metrics, particle, sandbox, water, weather}.rs (particle.rs carries apply_emitter_params, fed by the typed NIF emitter pipeline; sandbox.rs is sandbox_seat_system, M42)
Scene Setup:     byroredux/src/scene.rs (thin) → scene/{nif_loader, world_setup}.rs (+ *_tests.rs siblings: climate_tod_hours, cloud_tile_scale, procedural_fallback, radius_parse)
Render Data:     byroredux/src/render/ (mod.rs carries build_render_data + draw enumeration) → render/{camera, lights, skinned, static_meshes, particles, sky, water}.rs (+ *_tests.rs siblings)
Cell Loader:     byroredux/src/cell_loader.rs (thin dispatch) → cell_loader/{load, unload, exterior, references, spawn, partial, euler, refr, terrain, terrain_lod, object_lod, water, load_order, index, precombined, transition, nif_import_registry}.rs (+ *_tests.rs siblings)
Commands:        byroredux/src/commands/ (per-domain split #1323/TD9-NEW-03: mod.rs registry + world_info (help/stats/entities/systems/sys.accesses/mem.frag/ctx.scratch) + assets (tex.*/mesh.*/skin.*) + view (prid/cam.*/near/pick) + scene (light.*/door.teleport/script.activate/mat.*/ragdoll) + shared helpers) + byroredux/src/commands_tests.rs
NIFAL Translate: byroredux/src/material_translate.rs (translate_material — the SINGLE raw ImportedMesh → ECS Material boundary; per-game material classification happens here, never in the shader) + crates/core/src/ecs/components/material.rs (Material::resolve_pbr; canonical metalness/roughness are plain f32 fields, resolve-once). Spec: docs/engine/nifal.md. See also /audit-nifal.
EXAL Translate:  byroredux/src/env_translate.rs (EXAL exterior-environment translation boundary: terrain/sky/sun/weather/water/LOD). Spec: docs/engine/exal.md.
Ragdoll:         byroredux/src/ragdoll.rs (M41.x ragdoll activation + writeback; PHYSAL consumer). Spec: docs/engine/physal.md.
Cornell Harness: byroredux/src/cornell.rs (--cornell self-contained RT material/lighting reference scene; no on-disk game data)
Asset Provider:  byroredux/src/asset_provider.rs (BSA/BA2 texture+mesh extraction, resolve_texture, strip_build_prefix for AE pipeline-path paths)
Components:      byroredux/src/components.rs (binary-local markers + app resources: Spinning, AlphaBlend, TwoSided, DoorTeleport, IsFxMesh, IsLodTerrain, IsCollisionOnly, FootstepEmitter/Config/Scratch, CellLightingRes, SkyParamsRes, WeatherDataRes, LightTuning, …). Shared ECS components (WaterPlane/WaterVolume/SubmersionState) live in crates/core/src/ecs/components/water.rs; SelectedRef is a resource in crates/core/src/ecs/resources/mod.rs)
NPC Spawn:       byroredux/src/npc_spawn.rs           (M41 actor instantiation; M42.2 adds CTDA package-condition gating (`package_conditions_pass`, fail-open on unimplemented condition functions) + PLDT-radius resolution (`active_sandbox_location`) feeding SandboxBehavior)
Sandbox AI:      byroredux/src/systems/sandbox.rs (M42 sandbox_seat_system: nearest-free-seat assignment, per-marker reservation via SeatReservations keyed (furniture, marker index)) + crates/core/src/ecs/components/{sandbox, furniture}.rs (SandboxBehavior/Seated markers; Furniture BSFurnitureMarker entry positions, M41.5). v0 scope only — no target scoring/scheduling/meals/sleep/wander/ownership. Doc: docs/engine/npc-spawn-ai-packages.md.
World Stream:    byroredux/src/streaming.rs           (M40 cell lifecycle) + streaming_tests.rs
SF Smoke:        byroredux/src/sf_smoke.rs            (Starfield ESM resolve-rate harness, --sf-smoke CLI)
Golden Frames:   byroredux/tests/golden_frames.rs     (cube-demo frame-60 regression PNG; opts into --ignored)
Legacy Ref:      docs/legacy/
```

## Key Reference Docs

These docs are the authoritative, code-verified reference for their domain.
Prefer them over re-deriving facts from source during an audit.

| Doc | What it documents |
|-----|------------------|
| `docs/engine/shader-pipeline.md` | All 19 shaders, G-buffer attachment formats, `GpuCamera`/`GpuInstance`/`GpuMaterial`/`GpuLight` exact byte layouts, descriptor set bindings (Set 0–2), per-frame submission order, pipeline cache |
| `docs/engine/memory-budget.md` | VRAM/RAM ceilings, SSBO sizes, LRU eviction thresholds (`AccelerationManager`, `TextureRegistry`, BGSM cache, `MeshRegistry`), deferred-destroy countdown depth |
| `docs/engine/nifal.md` | NIFAL three-tier canonical translation spec (Imported* → translate() → Canonical); single-boundary / no-fabrication / no-render-time-fallback rules |
| `docs/engine/plugin-loading.md` | `PluginManifest` TOML schema, `DataStore`, `DependencyResolver` algorithm, Form ID three-layer design, ESM parser entry points, conflict resolution |
| `docs/engine/pipeline-overview.md` | Cross-cutting trace #1: a single interior cell load end-to-end, ESM record → ECS spawn → GPU draw |
| `docs/engine/exterior-grid-streaming.md` | Cross-cutting trace #2: exterior worldspace grid streaming — background pre-parse worker, cell-boundary crossing, door teleport scene swaps |
| `docs/engine/save-load-roundtrip.md` | Cross-cutting trace #3: M45/M45.1 save — what a snapshot captures, atomic disk write, live load-apply onto a *running* engine (no process restart) |
| `docs/engine/npc-spawn-ai-packages.md` | Cross-cutting trace #4: NPC_ spawn → AI package selection (CTDA gating, M42.2) → Sandbox behavior (M41.5/M42). States plainly which of the ~17 FO3/FNV package procedures actually execute at runtime (currently: Sandbox only) |
| `docs/feature-matrix.md` | What works at runtime per game — cell loading, rendering, NPCs, audio, scripting, physics, UI. Living status document. (NOTE: the "Scripting (M47)" + "Save / load (M45)" rows lag the code — M45/M45.1 + the M47.2 .pex slice shipped; treat the matrix as a floor, not ceiling, and flag the doc-rot.) |
| `docs/engine/scripting.md` | ECS-native scripting model (Papyrus VM → ECS), recognizer-chain design, what `.pex`/recognizers translate vs. defer. Paired with `docs/engine/papyrus-parser.md` (`.psc` AST), `docs/engine/m47-0-design.md`, `docs/engine/m47-2-design.md`, `docs/engine/m47-2-recognizer-scaling.md`. Owner audit: `/audit-scripting`. |
| `docs/contributing.md` | Prerequisites, build, test tiers (unit/integration/Vulkan/smoke), shader recompile, game data paths, CI jobs |

Crate count: 21 under `crates/` — audio, bgsm, bsa, core, cxx-bridge,
debug-protocol, debug-server, debug-ui, facegen, nif, papyrus, pex, physics,
platform, plugin, renderer, save, scripting, sfmaterial, spt, ui. Use this as a
coverage sanity check: an audit that never touches a relevant crate here is
incomplete. (`pex` + `save` are the two newest, added in Sessions 50–51 for the
M45 save/load and M47.2 compiled-Papyrus arcs — owned by `/audit-save` and
`/audit-scripting` respectively.)

## Game Data Locations

```
Oblivion:      /mnt/data/SteamLibrary/steamapps/common/Oblivion/Data/
Fallout 3:     /mnt/data/SteamLibrary/steamapps/common/Fallout 3 goty/Data/
Fallout NV:    /mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data/
Skyrim SE:     /mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data/
Fallout 4:     /mnt/data/SteamLibrary/steamapps/common/Fallout 4/Data/
Fallout 76:    /mnt/data/SteamLibrary/steamapps/common/Fallout76/Data/
Starfield:     /mnt/data/SteamLibrary/steamapps/common/Starfield/Data/
Gamebryo 2.3:  /media/matias/Respaldo 2TB/Start-Game/Leaks/Gamebryo_2.3 SRC/Gamebryo_2.3/
```

## Legacy Source (for compatibility audits)

```
CoreLibs/NiMain/       Scene graph, rendering, materials
CoreLibs/NiAnimation/  Controllers, interpolators, keyframes
CoreLibs/NiCollision/  OBB trees, raycasting
CoreLibs/NiSystem/     Memory, threading, I/O
SDK/Win32/Include/     1,592 public headers
```

## Severity Definitions

See `.claude/commands/_audit-severity.md` for the unified severity scale (CRITICAL / HIGH / MEDIUM / LOW).

## Methodology

- Be skeptical. Assume there are bugs even if the code "looks fine."
- For each claim, re-read the code path to confirm before including it.
- Prefer evidence from concrete code paths (call sites, data structures, configs) over assumptions.
- After making a finding, attempt to disprove it. Only include findings you cannot disprove.

## Rust-Specific Context Rules

- **Unsafe blocks**: Always read surrounding code and safety comment. Every unsafe MUST have justification.
- **Lifetimes**: When reading function signatures, trace caller lifetimes through borrows.
- **Trait bounds**: Check Send + Sync requirements on Component/Resource types.
- **Drop ordering**: Validate destroy-before-parent relationships (Vulkan objects).
- **Vulkan validation**: Reference Khronos spec for behavior guarantees.
- **Lock ordering**: Verify TypeId-sorted acquisition for multi-component queries.

## Context Management Rules

- **Max 1500 lines per Read** — use `offset` and `limit` to paginate larger files.
- **Grep before Read** — search for the specific pattern first, then read only relevant sections.
- **Incremental writes** — append findings to the report as you go; do not hold everything in memory.
- **One dimension at a time** — complete and write up one dimension before starting the next.

## Path-Reference Convention (post-`#1114`)

Backticked file/dir paths in any audit-*.md skill (or this file)
**must resolve against the live repository tree**. The validate gate
at `.claude/commands/_audit-validate.sh` enforces this and is the
structural fix for the recurring TD7-* stale-path findings.

- Backticks = "this path exists right now". The gate fails CI / the
  audit if it doesn't.
- Forward-looking refs (a file that doesn't yet exist) or
  backwards-looking refs (a file that was deleted) **must not** use
  backticks — write them as plain text or italics.
- Run `.claude/commands/_audit-validate.sh` before committing edits
  to any audit skill.

## Deduplication (MANDATORY)

Before reporting ANY finding:

1. Run: `gh issue list --repo matiaszanolli/ByroRedux --limit 200 --json number,title,state,labels` and save to `/tmp/audit/issues.json`
2. Search for keywords from your finding in existing issue titles
3. Scan `docs/audits/` for prior reports covering the same issue
4. If OPEN: note as "Existing: #NNN" and skip
5. If CLOSED: verify fix is in place. If regressed, report as "Regression of #NNN"
6. If no match: report as NEW

## Base Per-Finding Format

```
### <ID>: <Short Title>
- **Severity**: CRITICAL | HIGH | MEDIUM | LOW
- **Dimension**: <audit area>
- **Location**: `<file-path>:<line-range>`
- **Status**: NEW | Existing: #NNN | Regression of #NNN
- **Description**: What is wrong and why
- **Evidence**: Code snippet or exact call path demonstrating the issue
- **Impact**: What breaks, when, blast radius
- **Related**: Links to related findings or issues
- **Suggested Fix**: Brief direction (1-3 sentences)
```

Deep audit commands add extra fields (e.g., `Trigger Conditions`, `Flow`, `Changed File`) — see each command for details.

## Domain Labels

These are the labels that actually exist in the repo (verify drift with
`gh label list --repo matiaszanolli/ByroRedux`). `/audit-publish` must only
apply labels from this set — `gh issue create` rejects unknown labels.

Severity: `critical`, `high`, `medium`, `low`
Domain: `ecs`, `renderer`, `vulkan`, `pipeline`, `memory`, `sync`, `cxx`, `nif-parser`, `nif`, `import-pipeline`, `animation`, `legacy-compat`, `performance`, `safety`, `tech-debt`
Type: `bug`, `enhancement`, `documentation`

Subsystems without their own label map to the closest existing domain:
BSA/BA2/CSG and ESM/cell loading → `import-pipeline`; audio / platform /
SpeedTree / sfmaterial → `legacy-compat` or `tech-debt`. There is **no**
`bsa`, `esm`, `platform`, or `maintenance` label — do not apply them.

## Report Finalization

1. Save your report to: `docs/audits/AUDIT_<TYPE>_<TODAY>.md` (YYYY-MM-DD format)
2. Do NOT create GitHub issues directly
3. Inform the user the report is ready and suggest:
   ```
   /audit-publish docs/audits/AUDIT_<TYPE>_<TODAY>.md
   ```
