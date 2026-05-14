# Shared Audit Protocol — ByroRedux

This file is referenced by all audit skills. Do NOT use as a slash command (prefixed with `_`).

## Project Layout

```
Core ECS:        crates/core/src/ecs/
Components:      crates/core/src/ecs/components/
Animation:       crates/core/src/animation/          (types, player, stack, registry, interpolation, root_motion, text_events, controller)
Resources:       crates/core/src/ecs/resources.rs
Strings:         crates/core/src/string/
NIF Parser:      crates/nif/src/
NIF Blocks:      crates/nif/src/blocks/               (see blocks/mod.rs dispatch; controller/ subdir, particle.rs, shader.rs, tri_shape.rs, skin.rs, properties.rs, interpolator.rs, extra_data.rs, light.rs, multibound.rs, palette.rs, legacy_particle.rs, texture.rs, collision.rs, bs_geometry.rs, node.rs, base.rs, traits.rs; *_tests.rs siblings)
NIF Import:      crates/nif/src/import/               (mod.rs thin dispatch + types.rs + tests.rs; walk.rs, mesh.rs + mesh_*_tests.rs siblings, material/{mod, walker, shader_data, *_tests}, transform.rs, coord.rs, collision.rs)
NIF Animation:   crates/nif/src/anim.rs + anim/{types.rs, tests.rs}
BSA Reader:      crates/bsa/src/archive.rs
BA2 Reader:      crates/bsa/src/ba2.rs
BGSM Materials:  crates/bgsm/src/                     (FO4+ external material parser)
FaceGen (M41):   crates/facegen/src/                  (.tri/.egt morph + texture blend)
Physics (M28):   crates/physics/src/                  (Rapier3D bridge)
Papyrus (M30):   crates/papyrus/src/                  (.psc lexer + Pratt parser → AST)
Scripting (M12): crates/scripting/src/                (ECS-native events, timers, cleanup)
Audio (M44):     crates/audio/src/lib.rs + tests.rs   (byroredux-audio: kira backend, AudioWorld resource, AudioListener/AudioEmitter/OneShotSound components, audio_system, SoundCache, streaming music, global reverb send)
SpeedTree (S1):  crates/spt/src/                      (byroredux-spt: TLV walker for FNV/FO3/Oblivion .spt; placeholder-billboard import fallback)
Debug Protocol:  crates/debug-protocol/src/           (wire types, component registry)
Debug Server:    crates/debug-server/src/             (TCP server + DebugDrainSystem)
Renderer:        crates/renderer/src/vulkan/
VulkanContext:   crates/renderer/src/vulkan/context/  (mod.rs, draw.rs, resize.rs, resources.rs, helpers.rs, screenshot.rs)
Accel (RT):      crates/renderer/src/vulkan/acceleration.rs
G-Buffer:        crates/renderer/src/vulkan/gbuffer.rs
SVGF Denoiser:   crates/renderer/src/vulkan/svgf.rs
TAA (M37.5):     crates/renderer/src/vulkan/taa.rs
Composite:       crates/renderer/src/vulkan/composite.rs
SSAO:            crates/renderer/src/vulkan/ssao.rs
Caustics (M??):  crates/renderer/src/vulkan/caustic.rs
Volumetrics(M55):crates/renderer/src/vulkan/volumetrics.rs  (160×90×128 froxel grid, inject + integrate compute, single-ray TLAS shadow, HG phase)
Bloom (M58):     crates/renderer/src/vulkan/bloom.rs        (5-mip down + 4-mip up pyramid, B10G11R11_UFLOAT, 4-tap bilinear)
Water (M38):     crates/renderer/src/vulkan/water.rs        (WaterPipeline: vertex displacement + Fresnel, RT reflection/refraction against TLAS)
GPU Skin (M29):  crates/renderer/src/vulkan/skin_compute.rs
Material (R1):   crates/renderer/src/vulkan/material.rs   (MaterialBuffer SSBO, GpuMaterial dedup; replaces per-instance fields)
SPIR-V Reflect:  crates/renderer/src/vulkan/reflect.rs    (descriptor layout reflection from SPIR-V)
Scene Buffers:   crates/renderer/src/vulkan/scene_buffer.rs
Descriptors:     crates/renderer/src/vulkan/descriptors.rs
Vk Debug Util:   crates/renderer/src/vulkan/debug.rs
Vk Instance:     crates/renderer/src/vulkan/instance.rs
Vk Surface:      crates/renderer/src/vulkan/surface.rs
Mesh:            crates/renderer/src/mesh.rs
Vertex:          crates/renderer/src/vertex.rs
Tex Registry:    crates/renderer/src/texture_registry.rs (+ texture_registry_tests.rs)
Shaders:         crates/renderer/shaders/             (triangle.vert/frag, svgf_temporal.comp, taa.comp, composite.vert/frag, ssao.comp, cluster_cull.comp, skin_vertices.comp, caustic_splat.comp, volumetric_inject.comp, volumetric_integrate.comp, bloom_down.comp, bloom_up.comp, water.vert/frag, effect_lit.frag, ui.vert/frag)
Plugin/ESM:      crates/plugin/src/                   (esm/{mod, reader, sub_reader}, esm/cell/, esm/records/{actor, climate, container, global, items, misc/{water, character, world, ai, magic, effects, equipment}, mswp, pkin, scol, script, tree, weather, …}, record.rs generic dispatch; legacy/ holds the LegacyFormId/LoadOrder bridge — per-game stubs were removed under #390)
Platform:        crates/platform/src/
UI (Ruffle):     crates/ui/src/
CXX Bridge:      crates/cxx-bridge/
Binary:          byroredux/src/main.rs
Systems:         byroredux/src/systems.rs (27-line module index) → systems/{animation, audio, billboard, bounds, camera, debug, particle, water, weather}.rs
Scene Setup:     byroredux/src/scene.rs (thin) → scene/{nif_loader, world_setup}.rs (+ *_tests.rs siblings)
Render Data:     byroredux/src/render.rs (build_render_data, draw enumeration) + render/*_tests.rs siblings
Cell Loader:     byroredux/src/cell_loader.rs (thin dispatch) → cell_loader/{load, unload, exterior, references, spawn, partial, euler, refr, terrain, water, load_order, nif_import_registry}.rs (+ *_tests.rs siblings)
Commands:        byroredux/src/commands.rs + commands_tests.rs (console: help, stats, entities, tex.missing, light.dump, cam.where/pos/tp, prid, inspect, …)
Asset Provider:  byroredux/src/asset_provider.rs (BSA/BA2 texture+mesh extraction, resolve_texture, strip_build_prefix for AE pipeline-path paths)
Components:      byroredux/src/components.rs (markers + app resources: Spinning, AlphaBlend, TwoSided, Decal, WaterPlane, WaterVolume, SubmersionState, SelectedRef, FootstepScratch, …)
NPC Spawn:       byroredux/src/npc_spawn.rs           (M41 actor instantiation)
World Stream:    byroredux/src/streaming.rs           (M40 cell lifecycle) + streaming_tests.rs
SF Smoke:        byroredux/src/sf_smoke.rs            (Starfield ESM resolve-rate harness, --sf-smoke CLI)
Golden Frames:   byroredux/tests/golden_frames.rs     (cube-demo frame-60 regression PNG; opts into --ignored)
Legacy Ref:      docs/legacy/
```

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

Severity: `critical`, `high`, `medium`, `low`
Domain: `ecs`, `renderer`, `vulkan`, `pipeline`, `memory`, `sync`, `platform`, `cxx`, `nif`, `bsa`, `esm`, `animation`, `legacy-compat`, `performance`, `safety`, `tech-debt`
Type: `bug`, `enhancement`, `maintenance`

## Report Finalization

1. Save your report to: `docs/audits/AUDIT_<TYPE>_<TODAY>.md` (YYYY-MM-DD format)
2. Do NOT create GitHub issues directly
3. Inform the user the report is ready and suggest:
   ```
   /audit-publish docs/audits/AUDIT_<TYPE>_<TODAY>.md
   ```
