# ByroRedux Engine Documentation

This is the entry point for engine internals. If you came here from the
[README](../../README.md), it's the next layer down: how each subsystem is
built, where the code lives, and what guarantees it makes.

If you have not built or run the project yet, begin with
[Getting Started](../getting-started.md). This index assumes you already have
a working checkout and want to understand or change engine internals.

## Subsystems

| Doc | Crate(s) | What it covers |
|---|---|---|
| [Pipeline Overview](pipeline-overview.md) | all | Cross-cutting: one interior cell load traced end-to-end, CLI → parse → NIFAL → ECS spawn → scheduler → GPU upload → present |
| [Exterior Grid Streaming](exterior-grid-streaming.md) | byroredux, plugin, renderer | Cross-cutting: WRLD/LAND → terrain+REFRs, async pre-parse worker, multi-cell streaming, door-teleport cell swap |
| [Stream-Boundary State Continuity](stream-boundary-state-continuity.md) | byroredux, core | PROPOSED. One design for two flagged gaps sharing every hard part: EX-14/15's persistent-CELL "reconcile instead of re-spawn" and EX-16's actor/package snapshot-restore across ordinary despawn/respawn. FormID-keyed state (never raw `EntityId`), traced against real `Seated`/`TravelState`/`AmbientPackageRuntime` |
| [Save/Load Round-Trip](save-load-roundtrip.md) | save, byroredux | Cross-cutting: curated ECS snapshot, validation gates, atomic write + ring buffer, M45.1 live load-apply (FormId-keyed deltas + player-pose restore) |
| [NPC Spawn → AI Packages](npc-spawn-ai-packages.md) | byroredux, plugin, core | Cross-cutting: NPC_ spawn dispatch, PACK record parsing, package selection, and the currently runtime-backed procedures — honest about subsystem-blocked gaps. Documents the FO3/FNV flat-package path; see [PACKAL](packal.md) for the Skyrim+ tree/template path this doc predates. |
| [NAVM Pathfinding](navmesh-pathfinding.md) | byroredux, plugin, core | PROPOSED. A\* over the already-decoded `NavmRecord` triangle-adjacency graph + funnel string-pulling, feeding waypoints into the existing `step_toward` locomotion primitive; residency-scoped (only resident `NavmeshTile`s), degrades to straight-line beyond the known corridor. Door/cover triangle semantics confirmed undecoded on every game — flagged, not solved here. |
| [Architecture Overview](architecture.md) | all | Design principles, workspace layout, crate dependency graph |
| [ECS](ecs.md) | core | Components, storage backends, queries, scheduler, resources |
| [Vulkan Renderer](renderer.md) | renderer | Init chain, RT pipeline, multi-light, BLAS/TLAS, swapchain |
| [Renderer Evaluation](renderer-evaluation.md) | renderer, byroredux | Deterministic Cornell convergence captures, denoiser A/B, performance metadata, and comparison workflow |
| [Shader Pipeline](shader-pipeline.md) | renderer | All shader files, G-buffer layout, GPU data types (`GpuCamera`/`GpuInstance`/`GpuMaterial`/`GpuLight`), descriptor sets, pass order, pipeline cache |
| [Shadow Pipeline Trade-offs](shadow-pipeline-tradeoffs.md) | renderer | Alpha-era constants (`W_CLAMP`, TAA γ, M=8, 24-bit seed) with invalidation conditions |
| [FSR 3.1 Integration Plan](fsr3-upscaler-integration-plan.md) | renderer, fsr3-sys | Render/output extent split, temporal input contracts, mask policy, phase-by-phase execution record |
| [FSR 3.1 Troubleshooting](fsr3-troubleshooting.md) | renderer, fsr3-sys | Symptom→cause guide for the upscaler path: silent blit fallbacks, smearing, ghosting, FP16/FP32, bench gotchas |
| [Procedural Volumetric Fog](procedural-volumetric-fog.md) | renderer, plugin, nif | Froxel V-buffer, hybrid Z, temporal reprojection, TLAS/BLAS visibility, FSR masks, runtime knobs, and measurement table |
| [NIF Parser](nif-parser.md) | nif | Block-type dispatch (~254 arms, source of truth: `blocks/mod.rs`), version handling, robustness |
| [NIFAL — NIF Abstraction Layer](nifal.md) | nif, byroredux | `Imported*` → canonical ECS translation boundary; material, particle, collision, LOD slices |
| [EXAL — Exterior Abstraction Layer](exal.md) | byroredux, renderer | NIFAL mirror for outdoor rendering: terrain, sky, sun, weather, water, LOD |
| [EXAL — Ground Cover (procedural grass)](exal-groundcover.md) | byroredux, renderer | EXAL sub-doc. Deliberate divergence from the CE grass model: engine-authored density field + GPU scatter + vertex-shader blades; per-game `GRAS`/`LTEX` demoted to a palette hint |
| [EXAL — SpeedTree Full Rendering](exal-trees.md) | byroredux, spt, renderer | EXAL sub-doc, PROPOSED. Fills the near-field ring `exal.md` §5's distant LOD system already brackets: `.spt` geometry-tail decode plan (genuinely unstarted — two candidate tags, no confirmed layout), branch/frond + leaf-card import shape, RT/BLAS boundary, wind reusing ground cover's `WindField` |
| [PHYSAL — Physics Abstraction Layer](physal.md) | nif, physics, byroredux | Per-game Havok → one canonical articulated-physics spec → solver; ragdolls (Oblivion/FO3/FNV/Skyrim), double-ended (source game + backing solver) |
| [WATAL — Water Abstraction Layer](watal.md) | byroredux, renderer, physics | Per-game WATR/water → one canonical water state → render **and** solver; Skyrim-modelled, dumber games translate up; double-ended (render + physics); buoyancy/flow/swim/drown |
| [CHARAL — Character Abstraction Layer](charal.md) | core, plugin, byroredux | Per-game character **ruleset** (attributes/skills/perks/leveling) → one canonical character state (`ActorValues` + `CharacterLevel` + `Perks` + `Background`); 3 families (Fallout SPECIAL+XP, TES skills→level, Starfield mix); translates *rules*, not just data |
| [PACKAL — Package Abstraction Layer](packal.md) | plugin, scripting, byroredux | Unlike its `*AL` siblings, the parse-time fork is already unified (`PackRecord` carries both PACK shapes); the real gap is execution-time — Scene-driven package execution exists, ambient (non-Scene) execution didn't understand Skyrim+'s tree/template shape at all. Two slices shipped 2026-08-19: Sandbox (722 of 2,052 real `Skyrim.esm` NPCs resolve a radius, up from 0) and Patrol (514 more, 1,369 total now resolve some ambient behavior) — "Wander" was investigated as a third candidate and rejected: real content has no standalone Wander leaf (see packal.md §5). |
| [Archives (BSA + BA2 + CSG)](archives.md) | bsa | BSA v103/104/105, BA2 v1/2/3/7/8 GNRL + DX10, FO4 `.csg` precombined geometry |
| [Plugin Loading](plugin-loading.md) | plugin, core | `PluginManifest`, `DataStore`, `DependencyResolver`, Form ID system, ESM parser, conflict resolution |
| [Sandboxed Linked Mods](sandboxed-linked-mods.md) | mod-runtime; plugin, scripting, core, save, ui (planned links) | Implemented isolated Component Model host foundation plus requirements for typed engine/mod links, C++ portability, ECS barriers, persistence, and high-count profiles |
| [ESM Records](esm-records.md) | plugin | Cell loading, items, NPCs, factions, leveled lists |
| [Asset Pipeline](asset-pipeline.md) | byroredux, nif, bsa | TextureProvider, mesh cache, NIF→ECS import, BGSM/BGEM material merge |
| [Animation](animation.md) | core, nif | Keyframe pipeline, controllers, blending stack, GPU skinning |
| [Physics](physics.md) | physics | Rapier3D integration, NIF collision → ECS → stepper, player body |
| [Cell Lighting](lighting-from-cells.md) | byroredux | XCLL extraction, RT integration |
| [Physical Lighting Backbone](physical-lighting-backbone.md) | core, byroredux, renderer | Canonical units/emitters, explicit visibility layers, shared transport, adaptive ray allocation, temporal reconstruction, reference tests |
| [Material Abstraction](material-abstraction.md) | byroredux, nif | Per-game material translation to canonical `Material`; glass/PBR classify |
| [Per-game Translation Survey](per-game-translation-survey.md) | nif, byroredux | Game-specific quirks, shader variant coverage, known gaps per title |
| [UI System](ui.md) | ui | Scaleform/SWF via Ruffle, deferred texture upload |
| [Scripting](scripting.md) | scripting | ECS-native scripting (events, timers, condition evaluator, M47 arc) |
| [Papyrus Parser](papyrus-parser.md) | papyrus | `.psc` → AST parser (M30.2), language grammar |
| [Game Loop](game-loop.md) | byroredux | winit integration, frame loop, cell loading |
| [Coordinate System](coordinate-system.md) | nif, byroredux | Z-up→Y-up, CW rotations, transform composition |
| [String Interning](string-interning.md) | core | `FixedString`, `StringPool`, `Name` component |
| [C++ Interop](cxx-interop.md) | cxx-bridge | `cxx` crate bridge, FFI boundary |
| [Platform](platform.md) | platform | winit windowing, raw handles |
| [Launcher](launcher.md) | boot-request, game-detect, byro-launcher (all planned); core, bsa, plugin | PROPOSED. The public-facing front end: separate `eframe`/glow process (must render when Vulkan init fails), an intent-shaped `BootRequest` contract instead of a GUI over argv, Steam/GOG install detection + pre-launch archive validation, shared `SettingsRegistry` model with the in-game menu, save-slot sidecar metadata, per-game compatibility tiers |
| [Game Compatibility](game-compatibility.md) | all | Per-game parse rate matrix and known gaps |
| [FO4 CSG Format](fo4-csg-format.md) | bsa | `BSPackedGeomObject` TLV spec for FO4 precombined geometry |
| [Geometry Defect Triage](geometry-defect-triage-workflow.md) | nif, byroredux | Workflow for diagnosing and fixing mesh import artefacts |
| [Debug CLI](debug-cli.md) | debug-protocol, debug-server, byro-dbg | Live ECS inspection, Papyrus expression queries, screenshots |
| [Texture Upscale Workbench](../../tools/texture-upscale/README.md) | bsa, byro-texture-upscale | BSA/BA2 set discovery, external ESRGAN reference pass, semantic-map guided upscale |
| [Memory Budget](memory-budget.md) | renderer, bsa, byroredux | VRAM/RAM ceilings, SSBO sizes, LRU eviction thresholds, deferred-destroy queue |
| [Testing](testing.md) | all | Unit + integration test inventory, how to run |
| [Dependencies](dependencies.md) | all | Workspace crates and per-crate deps |

## Legacy reference

- [Gamebryo 2.3 Architecture](../legacy/gamebryo-2.3-architecture.md) — directory structure, class hierarchy, compatibility mapping
- [Key Source Files](../legacy/key-files.md) — paths to critical headers by subsystem
- [API Deep Dive](../legacy/api-deep-dive.md) — `NiObject`, `NiAVObject`, `NiStream`, `NiProperty`, `NiTransform`
- [Papyrus API Reference](../legacy/papyrus-api-reference.md) — what the script runtime needs to mirror
- [Creation Engine UI](../legacy/creation-engine-ui.md) — Scaleform menu architecture
- [Havok Constraint Layouts](../legacy/havok-constraint-layouts.md) — bhk* binary structures

## Quick Reference

| What | Where |
|---|---|
| ECS types | `byroredux_core::ecs::*` |
| Math (`Vec3`, `Quat`, etc.) | `byroredux_core::math::*` |
| String interning | `byroredux_core::string::{StringPool, FixedString}` |
| Form IDs | `byroredux_core::form_id::{FormId, PluginId, FormIdPool}` |
| Animation | `byroredux_core::animation::*` (`AnimationClipRegistry`, `AnimationPlayer`) |
| Vulkan context | `byroredux_renderer::VulkanContext` |
| Mesh registry | `byroredux_renderer::mesh::MeshRegistry` |
| Texture registry | `byroredux_renderer::texture_registry::TextureRegistry` |
| NIF parsing | `byroredux_nif::parse_nif`, `byroredux_nif::import::*` |
| BSA reader | `byroredux_bsa::BsaArchive` |
| BA2 reader | `byroredux_bsa::Ba2Archive` |
| ESM cell index | `byroredux_plugin::esm::cell::parse_esm_cells` |
| ESM record index | `byroredux_plugin::esm::records::parse_esm` |
| Plugin manifests | `byroredux_plugin::PluginManifest` |
| Sandboxed mod runtime | `byroredux_mod_runtime::{SandboxRuntime, SandboxConfig, Principal, CapabilitySet}` |
| Physics world | `byroredux_physics::PhysicsWorld` |
| Physics sync system | `byroredux_physics::physics_sync_system` |
| Window creation | `byroredux_platform::window::create_window` |
| C++ bridge | `byroredux_cxx_bridge::ffi::*` |
| Debug protocol | `byroredux_debug_protocol::{DebugRequest, DebugResponse}` |
| Debug server | `byroredux_debug_server::start()` |
| Debug CLI | `tools/byro-dbg` binary |
| Texture upscale workbench | `tools/texture-upscale` binary |

## Stats

Live counts (test totals, LOC, source files, parse-rate matrix,
bench-of-record) live in [ROADMAP.md → Project Stats](../../ROADMAP.md).
This doc deliberately does **not** mirror those numbers — every fact
should live in exactly one home, and ROADMAP is refreshed at every
session-close ritual.

For an at-the-keyboard live count:

```bash
cargo test --workspace --lib 2>&1 | grep "^test result:" | \
    awk '{s+=$4} END {print "tests:", s}'
find . -name "*.rs" -not -path "*/target/*" -not -path "*/tests/*" | \
    grep -v "/tests/" | xargs wc -l | tail -1
```

Per-game NIF parse-rate sweeps live in
[Game Compatibility](game-compatibility.md). ESM record categories
indexed are listed in [ESM Records](esm-records.md).

## Reading order

If you're new to the codebase, here's a sane reading path:

1. [Pipeline Overview](pipeline-overview.md) — the big picture first: one
   request traced end-to-end, with pointers into everything below
2. [Architecture Overview](architecture.md) — orient yourself in the workspace
3. [ECS](ecs.md) — the data model everything else hangs off
4. [Vulkan Renderer](renderer.md) — how frames get drawn
5. [NIF Parser](nif-parser.md) and [Archives](archives.md) — how raw bytes become geometry
6. [ESM Records](esm-records.md) — how raw bytes become world state
7. [Asset Pipeline](asset-pipeline.md) — how those two come together at cell load
8. [Game Loop](game-loop.md) — how the engine ties it all together at runtime

For a single-day onboarding, [Pipeline Overview](pipeline-overview.md) →
[Architecture](architecture.md) → [ECS](ecs.md) is enough to understand the
engine end-to-end.
