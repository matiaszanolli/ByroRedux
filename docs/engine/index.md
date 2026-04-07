# ByroRedux Engine Documentation

This is the entry point for engine internals. If you came here from the
[README](../../README.md), it's the next layer down: how each subsystem is
built, where the code lives, and what guarantees it makes.

## Subsystems

| Doc | Crate(s) | What it covers |
|---|---|---|
| [Architecture Overview](architecture.md) | all | Design principles, workspace layout, crate dependency graph |
| [ECS](ecs.md) | core | Components, storage backends, queries, scheduler, resources |
| [Vulkan Renderer](renderer.md) | renderer | Init chain, RT pipeline, multi-light, BLAS/TLAS, swapchain |
| [NIF Parser](nif-parser.md) | nif | 186 block types, version handling, robustness, parse-rate matrix |
| [Archives (BSA + BA2)](archives.md) | bsa | BSA v103/104/105, BA2 v1/2/3/7/8, GNRL + DX10 |
| [ESM Records](esm-records.md) | plugin | Cell loading, items, NPCs, factions, leveled lists |
| [Asset Pipeline](asset-pipeline.md) | byroredux, nif, bsa | TextureProvider, mesh cache, NIF→ECS import |
| [Animation](animation.md) | core, nif | Keyframe pipeline, controllers, blending stack |
| [Cell Lighting](lighting-from-cells.md) | byroredux | XCLL extraction, RT integration |
| [UI System](ui.md) | ui | Scaleform/SWF via Ruffle, deferred texture upload |
| [Game Loop](game-loop.md) | byroredux | winit integration, frame loop, cell loading |
| [Coordinate System](coordinate-system.md) | nif, byroredux | Z-up→Y-up, CW rotations, transform composition |
| [String Interning](string-interning.md) | core | `FixedString`, `StringPool`, `Name` component |
| [C++ Interop](cxx-interop.md) | cxx-bridge | `cxx` crate bridge, FFI boundary |
| [Platform](platform.md) | platform | winit windowing, raw handles |
| [Scripting](scripting.md) | scripting | ECS-native scripting (events, timers); contrast with Papyrus |
| [Game Compatibility](game-compatibility.md) | all | Per-game parse rate matrix and known gaps |
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
| Window creation | `byroredux_platform::window::create_window` |
| C++ bridge | `byroredux_cxx_bridge::ffi::*` |

## Stats

| Metric                              | Value          |
|-------------------------------------|----------------|
| Rust source files                   | 142            |
| Lines of Rust                       | ~35,800        |
| Workspace crates                    | 10             |
| Unit tests passing                  | 372            |
| Integration tests (`#[ignore]`'d)   | 14             |
| NIFs in per-game integration sweeps | 177,286        |
| Per-game NIF parse success rate     | 100% (7 games) |
| External dependency crates          | ~25            |

Numbers above are accurate as of M24 Phase 1 (April 2026). For the live
counts run `cargo test` and `cargo test --test parse_real_nifs -- --ignored`.

## Reading order

If you're new to the codebase, here's a sane reading path:

1. [Architecture Overview](architecture.md) — orient yourself in the workspace
2. [ECS](ecs.md) — the data model everything else hangs off
3. [Vulkan Renderer](renderer.md) — how frames get drawn
4. [NIF Parser](nif-parser.md) and [Archives](archives.md) — how raw bytes become geometry
5. [ESM Records](esm-records.md) — how raw bytes become world state
6. [Asset Pipeline](asset-pipeline.md) — how those two come together at cell load
7. [Game Loop](game-loop.md) — how the engine ties it all together at runtime

For a single-day onboarding, [Architecture](architecture.md) → [ECS](ecs.md) → [Game Loop](game-loop.md) is enough to understand the engine end-to-end.
