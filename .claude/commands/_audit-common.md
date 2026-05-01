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
NIF Blocks:      crates/nif/src/blocks/               (30 modules — see blocks/mod.rs dispatch; controller/, particle/, shader/, tri_shape/, skin/, properties/, light, multibound, palette, legacy_particle, …)
NIF Import:      crates/nif/src/import/               (walk, mesh, material/, transform, coord, collision)
NIF Animation:   crates/nif/src/anim.rs
BSA Reader:      crates/bsa/src/archive.rs
BA2 Reader:      crates/bsa/src/ba2.rs
BGSM Materials:  crates/bgsm/src/                     (FO4+ external material parser)
FaceGen (M41):   crates/facegen/src/                  (.tri/.egt morph + texture blend)
Physics (M28):   crates/physics/src/                  (Rapier3D bridge)
Papyrus (M30):   crates/papyrus/src/                  (.psc lexer + Pratt parser → AST)
Scripting (M12): crates/scripting/src/                (ECS-native events, timers, cleanup)
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
Tex Registry:    crates/renderer/src/texture_registry.rs
Shaders:         crates/renderer/shaders/             (triangle.vert/frag, svgf_temporal.comp, taa.comp, composite.vert/frag, ssao.comp, cluster_cull.comp, skin_vertices.comp, caustic_splat.comp, ui.vert/frag)
Plugin/ESM:      crates/plugin/src/                   (esm/{mod, reader, sub_reader}, esm/cell/, esm/records/{actor, climate, container, global, items, misc, mswp, pkin, scol, script, weather, …}, record.rs generic dispatch, legacy/ TES3-FO4 stubs)
Platform:        crates/platform/src/
UI (Ruffle):     crates/ui/src/
CXX Bridge:      crates/cxx-bridge/
Binary:          byroredux/src/main.rs
Systems:         byroredux/src/systems.rs
Scene Setup:     byroredux/src/scene.rs
Render Data:     byroredux/src/render.rs
Cell Loader:     byroredux/src/cell_loader.rs + cell_loader_refr.rs + cell_loader_terrain.rs (+ ~10 *_tests.rs siblings)
NPC Spawn:       byroredux/src/npc_spawn.rs           (M41 actor instantiation)
World Stream:    byroredux/src/streaming.rs           (M40 cell lifecycle)
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
Domain: `ecs`, `renderer`, `vulkan`, `pipeline`, `memory`, `sync`, `platform`, `cxx`, `nif`, `bsa`, `esm`, `animation`, `legacy-compat`, `performance`, `safety`
Type: `bug`, `enhancement`, `maintenance`

## Report Finalization

1. Save your report to: `docs/audits/AUDIT_<TYPE>_<TODAY>.md` (YYYY-MM-DD format)
2. Do NOT create GitHub issues directly
3. Inform the user the report is ready and suggest:
   ```
   /audit-publish docs/audits/AUDIT_<TYPE>_<TODAY>.md
   ```
