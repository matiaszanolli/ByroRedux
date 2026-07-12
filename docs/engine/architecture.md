# ByroRedux — Architecture Overview

ByroRedux is a clean rebuild of the Gamebryo and Creation engine lineage,
built from scratch in Rust and C++ using Vulkan. The goal is a modern engine
with Rust's safety guarantees that can load and render content from every
Bethesda Gamebryo/Creation game (Oblivion through Starfield).

> **Doc currency.** This overview was fully reconciled against the tree on
> 2026-05-28 (post-Session-42). Workspace-wide figures (test counts, file
> counts, the compat matrix, bench FPS) are authoritative in
> [`ROADMAP.md`](../../ROADMAP.md#project-stats), which `/session-close`
> refreshes every session; the snapshots quoted here are allowed to drift one
> sweep behind.

## Workspace Structure

The workspace is **21 members**: 19 library crates under `crates/`, the
`byroredux` binary, and the `tools/byro-dbg` debug CLI.

```
byroredux/
├── Cargo.toml                 Workspace root (21 members)
├── byroredux/                 Binary crate — game loop, scene setup, cell loader,
│                              world streaming, NPC spawn/equip, fly + character
│                              controllers, animation/transform/render systems
├── crates/
│   ├── core/                  ECS, math (glam), animation engine, types,
│   │                          string interning, form IDs, components/resources,
│   │                          console-command registry
│   ├── plugin/                Plugin system, ESM/ESP parser (cell + records),
│   │                          manifests, conflict resolution, legacy bridge
│   ├── nif/                   NIF file parser + animation importer, scene import
│   │                          to ECS-friendly meshes (NIFAL canonical translation)
│   ├── spt/                   SpeedTree (.spt) TLV parser + placeholder-billboard
│   │                          import (Phase 1)
│   ├── bsa/                   BSA + BA2 archive readers for every Bethesda game
│   │                          (Oblivion through Starfield)
│   ├── bgsm/                  BGSM / BGEM material readers (FO4+) + template merge
│   ├── sfmaterial/            Starfield .mat / layered-material reader
│   ├── facegen/               FaceGen .egm / .egt / .tri morph readers + eval
│   ├── physics/               Rapier3D-backed physics simulation + kinematic
│   │                          character controller, consumes NIF collision data
│   ├── renderer/              Vulkan graphics via ash + gpu-allocator, with RT
│   │                          extensions (VK_KHR_ray_query)
│   ├── audio/                 kira-backed audio (spatial sub-tracks, music, reverb)
│   ├── ui/                    Scaleform/SWF UI system (Ruffle integration)
│   ├── scripting/             ECS-native scripting (events, timers, conditions,
│   │                          Papyrus-demo hand-translations)
│   ├── papyrus/               Papyrus language parser (.psc source → AST)
│   ├── platform/              Windowing via winit (Linux-first)
│   ├── cxx-bridge/            C++ interop via cxx
│   ├── debug-protocol/        Wire types + component registry for the debug CLI
│   ├── debug-server/          TCP debug server embedded in the engine
│   └── debug-ui/              In-engine egui overlay (metrics, load queue)
├── tools/
│   └── byro-dbg/              Standalone debug CLI binary (TCP client, REPL)
└── docs/
    ├── engine/                This documentation
    ├── legacy/                Gamebryo 2.3 / Creation Engine analysis
    └── smoke-tests/           Manual end-to-end checks (device + on-disk data)
```

See [crates/core/src/ecs/](../../crates/core/src/ecs/) for the ECS implementation,
[crates/nif/src/blocks/](../../crates/nif/src/blocks/) for the NIF block parsers,
and [crates/renderer/src/vulkan/](../../crates/renderer/src/vulkan/) for the
renderer.

> **Submodule split (Sessions 34–36, 2026-05-10..14).** Most files that grew
> past ~2000 LOC were split into submodule directories. The biggest moves that
> matter for navigation: `byroredux/src/systems.rs` → `systems/`
> (animation / particle / character / camera / audio / weather / …),
> `byroredux/src/render.rs` → `render/` (static_meshes / skinned / particles /
> lights / sky / water / camera), `crates/nif/src/blocks/collision.rs` →
> `collision/` (9 siblings), `crates/nif/src/import/mesh/` and `.../anim/`,
> `crates/core/src/animation/`, `crates/renderer/src/vulkan/acceleration/` +
> `.../scene_buffer/` + `.../context/`, `crates/plugin/src/esm/cell/`, and
> `byroredux/src/cell_loader/`. A few thin `*.rs` re-export shims remain beside
> their directories (e.g. `byroredux/src/systems.rs` is now a 33-line module
> wrapper), while `scene.rs` and `cell_loader.rs` survive as substantial files
> alongside their companion directories.

## Design Principles

### 1. ECS over scene graph

The legacy Gamebryo engine uses a hierarchical scene graph where `NiAVObject`
is a God Object bundling transforms, properties, bounds, collision, and flags.
Redux decomposes these into independent components: `Transform`,
`GlobalTransform`, `Parent`, `Children`, `MeshHandle`, `Material`,
`LightSource`, `WorldBound`, `LocalBound`, `SkinnedMesh`, and gameplay data
like `Inventory` / `EquipmentSlots`. Systems that only need transforms never
touch collision data, and the renderer never sees AI state. The current
component set lives in
[crates/core/src/ecs/components/](../../crates/core/src/ecs/components/).

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

### 3. Interior mutability via RwLock + parallel dispatch

Every component storage and resource is wrapped in `RwLock`. Query methods
take `&self`, not `&mut self`. This enables multiple systems to read
different component types simultaneously, and is the foundation for the
parallel system dispatch that **M27 turned on** (closed 2026-05-23). Within a
[`Stage`](../../crates/core/src/ecs/scheduler.rs) the non-exclusive systems run
in parallel via rayon (behind the `parallel-scheduler` feature); exclusive
systems run sequentially after the parallel batch. Multi-component queries
acquire locks in `TypeId` order to prevent deadlocks, and a `lock_tracker`
([crates/core/src/ecs/lock_tracker.rs](../../crates/core/src/ecs/lock_tracker.rs))
catches ordering violations in debug builds.

Contention is made diagnosable up-front by **declared access** (R7,
[crates/core/src/ecs/access.rs](../../crates/core/src/ecs/access.rs)): systems
opt in via `Access::new().reads::<T>().writes::<U>()`, registered through
`Scheduler::add_to_with_access`. `Scheduler::access_report()` walks the
per-stage conflict graph (`None` / `Conflict { pairs }` / `Unknown`) and is
surfaced through the `sys.accesses` console command. After the M27 migration the
12 parallel-stage systems report **0 unknown / 0 conflicts**; runtime-mutually-
exclusive systems (audio, spin, the character-mode dispatcher, the
`player_controller_system` PlayerMode branch) were re-staged as exclusive.

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
memory with proper HOST→AS_BUILD memory barriers. Vulkan 1.3 `synchronization2`
is enabled (landed with the M28.5 character-controller work).

See [Vulkan Renderer](renderer.md) for the per-module breakdown.

### 7. Per-game data, one canonical representation (NIFAL)

A standing directive of the project is **never branch per-game in the shader or
renderer** — translate at the parser boundary instead
([`feedback_format_translation.md`](../../CLAUDE.md), the GameVariant pattern in
[`per-game-translation-survey.md`](per-game-translation-survey.md)). The
**NIF Abstraction Layer (NIFAL)** formalises this as a three-tier model and is
the cornerstone of cross-game compatibility:

```
              parse                  translate()                consume
  NIF bytes ─────────▶  Imported*  ─────────────▶  Canonical  ─────────▶  ECS / renderer
            (per-game raw decode    (resolved, game-agnostic,            / gameplay
             in crates/nif/blocks)   single convention)
```

NIFAL opened 2026-05-28 (post-Session-42), generalising the earlier
material-only work in
[`material-abstraction.md`](material-abstraction.md). The first slices landed
this session: the **material** slice resolves PBR to plain `f32` at the
`translate_material` boundary in
[`byroredux/src/material_translate.rs`](../../byroredux/src/material_translate.rs)
(no `Option` "resolve-it-later" payloads downstream); the **particle** slice
authors `NiPSysEmitter` base + birth rate + grow/fade size; plus a
node-passthrough triage and a collision audit (`BhkMultiSphereShape` and
`BhkConvexListShape` now translate). The design doc is
[`nifal.md`](nifal.md).

### 8. Linux-first, multiplatform later

The primary development platform is Linux. Platform abstractions exist in
the `platform` crate to enable future Windows/macOS support without
touching engine internals. The reference development machine is an AMD
Ryzen 9 7950X with an NVIDIA RTX 4070 Ti (12 GB) running Wayland.

## Crate Dependency Graph

`byroredux-core` is the spine — most engine crates depend on it for ECS types,
math, animation, string interning, form IDs, and the console registry. A second
small hub is `byroredux-bsa`, which the asset-format crates depend on so they
can pull bytes straight out of archives. The internal edges (verified against
each crate's `Cargo.toml`):

```
byroredux (binary)  →  core, renderer, platform, scripting, nif, spt, bsa,
                       bgsm, sfmaterial, facegen, audio, physics, plugin,
                       cxx-bridge, ui, debug-ui (+ debug-server, optional)

core            → (no internal deps — spine)
bsa             → (no internal deps — leaf)
cxx-bridge      → (no internal deps — leaf)
papyrus         → (no internal deps — leaf)
debug-protocol  → (no internal deps — leaf)

platform        → core
plugin          → core
physics         → core
renderer        → core, platform
ui              → core
nif             → core            (+ bsa as a dev-dependency, see below)
spt             → core, nif, bsa
bgsm            → bsa
sfmaterial      → bsa
facegen         → bsa
audio           → core, bsa
scripting       → core, plugin
debug-server    → core, papyrus, debug-protocol
debug-ui        → core, renderer
```

`byroredux-physics` depends only on `core`, keeping `rapier3d` / `nalgebra`
out of every other crate's build graph. The NIF parser's only *runtime*
internal dependency is `core`; it pulls in `byroredux-bsa` as a plain
**dev-dependency** (declared directly under `[dev-dependencies]` in
[`crates/nif/Cargo.toml`](../../crates/nif/Cargo.toml))
so the integration tests in
[`crates/nif/tests/parse_real_nifs.rs`](../../crates/nif/tests/parse_real_nifs.rs)
can walk both BSA and BA2 archives through a unified `MeshArchive` enum
(defined in `crates/nif/tests/common/mod.rs`).

## Subsystem map

Where each major capability lives:

| Capability | Crate(s) | Source root |
|---|---|---|
| ECS world / queries / scheduler / declared access | `core` | [crates/core/src/ecs/](../../crates/core/src/ecs/) |
| Math types (Vec/Mat/Quat) + coord conversion | `core` | [crates/core/src/math/](../../crates/core/src/math/) |
| Form IDs and plugin identity | `core` | [crates/core/src/form_id.rs](../../crates/core/src/form_id.rs) |
| Animation engine (clips, players, blending, controllers) | `core` | [crates/core/src/animation/](../../crates/core/src/animation/) |
| String interning | `core` | [crates/core/src/string/](../../crates/core/src/string/) |
| Plugin manifests + DataStore | `plugin` | [crates/plugin/src/](../../crates/plugin/src/) |
| ESM cell + record parsing | `plugin` | [crates/plugin/src/esm/](../../crates/plugin/src/esm/) |
| Legacy ESM/ESP/ESL bridge | `plugin` | [crates/plugin/src/legacy/](../../crates/plugin/src/legacy/) |
| NIF binary parser | `nif` | [crates/nif/src/blocks/](../../crates/nif/src/blocks/) |
| NIF→ECS scene import (NIFAL) | `nif` | [crates/nif/src/import/](../../crates/nif/src/import/) |
| KF animation import | `nif` | [crates/nif/src/anim/](../../crates/nif/src/anim/) |
| BSA reader (v103/104/105) | `bsa` | [crates/bsa/src/archive/](../../crates/bsa/src/archive/) |
| BA2 reader (v1/2/3/7/8) | `bsa` | [crates/bsa/src/ba2.rs](../../crates/bsa/src/ba2.rs) |
| BGSM/BGEM + Starfield materials | `bgsm`, `sfmaterial` | [crates/bgsm/src/](../../crates/bgsm/src/), [crates/sfmaterial/src/](../../crates/sfmaterial/src/) |
| FaceGen morph readers | `facegen` | [crates/facegen/src/](../../crates/facegen/src/) |
| SpeedTree (.spt) parse + import | `spt` | [crates/spt/src/](../../crates/spt/src/) |
| Physics world + kinematic controller | `physics` | [crates/physics/src/](../../crates/physics/src/) |
| Vulkan context (init, draw, resize) | `renderer` | [crates/renderer/src/vulkan/context/](../../crates/renderer/src/vulkan/context/) |
| Mesh / texture registries | `renderer` | [crates/renderer/src/](../../crates/renderer/src/) |
| RT acceleration structures | `renderer` | [crates/renderer/src/vulkan/acceleration/](../../crates/renderer/src/vulkan/acceleration/) |
| Per-frame scene SSBO/UBO + lights | `renderer` | [crates/renderer/src/vulkan/scene_buffer/](../../crates/renderer/src/vulkan/scene_buffer/) |
| Audio (kira, spatial, music, reverb) | `audio` | [crates/audio/src/](../../crates/audio/src/) |
| Ruffle/SWF UI | `ui` | [crates/ui/src/](../../crates/ui/src/) |
| ECS-native scripting + condition eval | `scripting` | [crates/scripting/src/](../../crates/scripting/src/) |
| Papyrus `.psc` → AST | `papyrus` | [crates/papyrus/src/](../../crates/papyrus/src/) |
| Debug protocol / server / overlay | `debug-protocol`, `debug-server`, `debug-ui` | [crates/debug-server/src/](../../crates/debug-server/src/) |
| Game loop, cell loader, world streaming | `byroredux` | [byroredux/src/](../../byroredux/src/) |

## Current State

ByroRedux loads cells from every Bethesda Gamebryo/Creation game and renders
them with RT shadows, reflections, and denoised indirect lighting. Cells
populate ECS entities with mesh handles, materials, lights, collision shapes,
and (for actors) `Inventory` / `EquipmentSlots`; transforms compose REFR
(placed reference) data with NIF-internal local transforms via the documented
[coordinate system](coordinate-system.md) pipeline. The ESM parser extracts
items, NPCs, factions, leveled lists, globals, game settings, quests (QUST
stages/objectives), and perk entries on top of cell + static extraction. World
streaming swaps cells in/out around the player; a kinematic character
controller walks the loaded geometry.

Workspace-wide metrics (ground truth lives in
[`ROADMAP.md`](../../ROADMAP.md#project-stats), refreshed each
`/session-close`; snapshot as of the Session 42 close, 2026-05-28):

| Metric                              | Value          |
|-------------------------------------|----------------|
| Rust source files (`.rs`, excl. `target/`) | 549 (518 outside `tests/` dirs) |
| Workspace members                   | 21 (19 crates + `byroredux` + `byro-dbg`) |
| Tests passing                       | ~2635          |
| NIFs in per-game integration sweeps | 184,886        |
| Per-game NIF clean-parse rate       | 100% on FO3 / FNV / Skyrim SE / FO4 / FO76; Oblivion 99.93%, Starfield 99.64% aggregate (recoverable 100% on all games; sweep 2026-07-11, #1900) |
| Supported archive formats           | BSA v103/104/105, BA2 v1/2/3/7/8 |

What works today, end-to-end:

- Open a Vulkan window on Linux (Wayland or X11), validation layers in debug.
- Load full interior **and** exterior cells from every supported game and
  render them with RT-shadowed multi-light. Reference benches (R6a-stale-13,
  2026-05-28): FNV Prospector Saloon 3507 entities @ ~71 FPS, Skyrim SE
  WhiterunBanneredMare 3211 entities @ ~330 FPS on an RTX 4070 Ti. Exact
  repro commands and the latest numbers are in
  [`ROADMAP.md`](../../ROADMAP.md#project-stats).
- Per-mesh NiLight sources contribute to the GpuLight buffer with per-light
  falloff and candle/chandelier flicker — Oblivion torches and candles light
  their surroundings.
- Simulate physics on the loaded cell via Rapier3D; walk it with the M28.5
  kinematic character controller (gravity, collide-and-slide, jump, autostep;
  `T` toggles fly/walk).
- Stream cells around the player and traverse door teleporters
  (interior↔interior and interior↔exterior).
- Load Skyrim SE meshes from BSA v105 (LZ4) and FO4 / FO76 / Starfield meshes
  from BA2 (v1/v2/v3/v7/v8); Starfield bring-up reached a walkable Cydonia.
- GPU-skinned NPCs animate and contribute to RT (per-skinned-entity BLAS
  refit); `.kf` animations play on named ECS entities with the full
  controller stack.
- Spawn and equip NPCs (ARMO/ARMA worn-mesh chain + LVLI dispatch + FaceGen).
- Sky, sun arc, time-of-day, four cloud layers, fog, and weather fade
  transitions (M33/M33.1); water with RT reflection/refraction and caustics
  (M38 + #1210).
- Render Scaleform SWF menus via Ruffle as overlay quads; in-engine egui
  debug overlay (metrics, GPU per-pass timings, load queue).
- Parse Papyrus `.psc` source to a full AST (M30.2) and run hand-translated
  ECS-native scripts with a CTDA condition evaluator (M47.0/M47.1).
- Walk a large mesh archive in seconds release-mode and assert per-game parse
  rates as integration telemetry.
