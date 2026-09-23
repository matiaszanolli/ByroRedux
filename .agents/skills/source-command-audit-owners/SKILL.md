---
name: "source-command-audit-owners"
description: "Migrated source command `_audit-owners`"
---

# source-command-audit-owners

Use this skill when the user asks to run the migrated source command `_audit-owners`.

## Command Template

# Audit Ownership Map — ByroRedux

Single source of truth for **path → owning audit → risk floor**. Read by `/audit-incremental` (routing), `/audit-suite` (coverage) and `_audit-validate.sh` (flags any crate, tool or sizeable `byroredux/src` module with no owner, and any row whose path no longer exists). Not a slash command.

Rules: rows are **path prefixes**; the FIRST matching row routes a changed file, and every owner listed on it applies. Specific rows precede general ones — keep that order. `Dim N` narrows an owner to one dimension. **Risk** is the floor severity for an un-disproven finding there. Per-game audits (`<game>`) own only *their title's data through the shared mechanism*. Adding a crate or a >300-LOC module means adding a row here (the validator will say so).

| Path prefix | Owner audit(s) | Risk |
|---|---|---|
| `crates/renderer/src/vulkan/groundcover` | exterior, renderer | HIGH |
| `crates/renderer/shaders/groundcover_` | exterior, renderer | HIGH |
| `crates/renderer/shaders/include/groundcover_` | exterior, renderer | HIGH |
| `crates/renderer/src/vulkan/sky_` | exterior, renderer | HIGH |
| `crates/renderer/shaders/include/sky.glsl` | exterior, renderer | HIGH |
| `crates/renderer/shaders/include/clouds.glsl` | exterior, renderer | HIGH |
| `crates/renderer/src/vulkan/water` | renderer Dim 8, exterior | HIGH |
| `crates/renderer/shaders/water.` | renderer Dim 8, exterior | HIGH |
| `crates/renderer/src/vulkan/acceleration/` | renderer, safety, concurrency | HIGH |
| `crates/renderer/src/vulkan/scene_buffer/` | renderer, nifal | HIGH |
| `crates/renderer/src/vulkan/material` | renderer, nifal | HIGH |
| `crates/renderer/src/vulkan/frame_upscaler.rs` | renderer Dim 11, safety Dim 1 | HIGH |
| `crates/renderer/src/vulkan/upscaling.rs` | renderer Dim 11, safety Dim 1 | HIGH |
| `crates/renderer/src/vulkan/presentation.rs` | renderer Dim 11 | HIGH |
| `crates/renderer/src/vulkan/exposure.rs` | renderer Dim 11 | HIGH |
| `crates/renderer/src/vulkan/egui_pass.rs` | renderer, concurrency | MEDIUM |
| `crates/renderer/src/vulkan/` | renderer, safety, concurrency, performance | HIGH |
| `crates/renderer/src/texture_registry` | renderer, performance | MEDIUM |
| `crates/renderer/` | renderer, performance | HIGH |
| `crates/fsr3-sys/` | renderer Dim 11, safety Dim 1 | HIGH |
| `crates/core/src/character/` | character | HIGH |
| `crates/core/src/combat.rs` | character | MEDIUM |
| `crates/core/src/stealth.rs` | character | MEDIUM |
| `crates/core/src/animation/` | ecs Dim 9, nif, nifal | MEDIUM |
| `crates/core/src/ecs/components/groundcover` | exterior | MEDIUM |
| `crates/core/src/ecs/components/water.rs` | exterior, physics | MEDIUM |
| `crates/core/src/ecs/components/restoration.rs` | gameplay | MEDIUM |
| `crates/core/src/ecs/components/inventory.rs` | gameplay | MEDIUM |
| `crates/core/src/ecs/components/lock.rs` | gameplay | MEDIUM |
| `crates/core/src/ecs/components/{sandbox,wander,travel,follow,escort,guard,patrol}.rs` | gameplay | MEDIUM |
| `crates/core/src/lighting.rs` | renderer, nifal | MEDIUM |
| `crates/core/src/atomic_file.rs` | save, tooling | MEDIUM |
| `crates/core/` | ecs, concurrency | HIGH |
| `crates/nif/src/import/material/` | nifal, nif | HIGH |
| `crates/nif/src/import/collision/` | physics, nif | HIGH |
| `crates/nif/` | nif; per-game | HIGH |
| `crates/plugin/` | esm; per-game | HIGH |
| `crates/bsa/` | parsers; per-game | HIGH |
| `crates/bgsm/` | parsers, nifal; fo4 | MEDIUM |
| `crates/sfmaterial/` | parsers; starfield | MEDIUM |
| `crates/hkx/` | parsers; scripting Dim 5 | MEDIUM |
| `crates/facegen/` | parsers; skyrim | MEDIUM |
| `crates/game-detect/` | parsers, tooling | MEDIUM |
| `crates/menuxml/` | ui, parsers | MEDIUM |
| `crates/spt/` | speedtree | MEDIUM |
| `crates/physics/` | physics, safety | HIGH |
| `crates/scripting/` | scripting | MEDIUM |
| `crates/pex/` | papyrus | MEDIUM |
| `crates/papyrus/` | papyrus | MEDIUM |
| `crates/save/` | save | MEDIUM |
| `crates/audio/` | audio | MEDIUM |
| `crates/ui/` | ui, safety | MEDIUM |
| `crates/mod-runtime/` | safety Dim 8 | MEDIUM |
| `crates/sdk/` | tooling | MEDIUM |
| `crates/debug-server/` | tooling, concurrency Dim 7 | MEDIUM |
| `crates/debug-protocol/` | tooling | MEDIUM |
| `crates/debug-ui/` | tooling | LOW |
| `crates/boot-request/` | tooling | MEDIUM |
| `crates/settings-io/` | tooling | MEDIUM |
| `crates/cxx-bridge/` | tech-debt (placeholder) | LOW |
| `crates/platform/` | tech-debt (placeholder) | LOW |
| `tools/` | tooling | LOW |
| `byroredux/src/env_translate` | exterior, nifal (mirror) | HIGH |
| `byroredux/src/groundcover_translate` | exterior | MEDIUM |
| `byroredux/src/fog.rs` | exterior, renderer | MEDIUM |
| `byroredux/src/cell_loader/terrain` | exterior; per-game | MEDIUM |
| `byroredux/src/cell_loader/water.rs` | exterior; per-game | MEDIUM |
| `byroredux/src/cell_loader/lod` | exterior, performance | MEDIUM |
| `byroredux/src/cell_loader/object_lod.rs` | exterior, performance | MEDIUM |
| `byroredux/src/cell_loader/placement_lod.rs` | exterior, performance | MEDIUM |
| `byroredux/src/cell_loader/reference_state.rs` | gameplay, save | MEDIUM |
| `byroredux/src/cell_loader/stream_snapshot.rs` | save, performance | MEDIUM |
| `byroredux/src/cell_loader` | per-game; performance | MEDIUM |
| `byroredux/src/material_translate.rs` | nifal | HIGH |
| `byroredux/src/ragdoll` | physics | HIGH |
| `byroredux/src/npc_spawn` | gameplay, performance Dim 7, concurrency Dim 7 | MEDIUM |
| `byroredux/src/systems/water.rs` | exterior, physics | MEDIUM |
| `byroredux/src/systems/weather.rs` | exterior | MEDIUM |
| `byroredux/src/systems/character.rs` | physics Dim 4+5; gameplay | MEDIUM |
| `byroredux/src/systems/audio.rs` | audio | MEDIUM |
| `byroredux/src/systems/{sandbox,wander,travel,follow,escort,guard,patrol,walk_anim,combat_ai,locomotion,navmesh_path,restoration}.rs` | gameplay | MEDIUM |
| `byroredux/src/systems/cinematic.rs` | scripting | MEDIUM |
| `byroredux/src/systems/` | ecs, performance | MEDIUM |
| `byroredux/src/render/` | renderer, performance | MEDIUM |
| `byroredux/src/boot/` | concurrency Dim 4, ecs Dim 5, tooling | HIGH |
| `byroredux/src/scheduler_access_tests.rs` | concurrency, ecs | HIGH |
| `byroredux/src/save_io` | save | MEDIUM |
| `byroredux/src/combat.rs` | gameplay | MEDIUM |
| `byroredux/src/inventory.rs` | gameplay | MEDIUM |
| `byroredux/src/interaction.rs` | gameplay, ui | MEDIUM |
| `byroredux/src/settings_io.rs` | tooling, gameplay | LOW |
| `byroredux/src/loading_screen.rs` | gameplay, esm | LOW |
| `byroredux/src/notifications.rs` | gameplay | LOW |
| `byroredux/src/hud.rs` | ui, gameplay | MEDIUM |
| `byroredux/src/scaleform_hud.rs` | ui | MEDIUM |
| `byroredux/src/ui_input.rs` | ui | MEDIUM |
| `byroredux/src/streaming` | performance Dim 7, concurrency Dim 7, exterior | MEDIUM |
| `byroredux/src/extensions/` | safety Dim 8, tooling | MEDIUM |
| `byroredux/src/studio_host.rs` | tooling | MEDIUM |
| `byroredux/src/commands` | ecs, tooling | MEDIUM |
| `byroredux/src/asset_provider/animation.rs` | scripting Dim 5, parsers | MEDIUM |
| `byroredux/src/asset_provider/audio.rs` | audio | MEDIUM |
| `byroredux/src/asset_provider/script.rs` | scripting, papyrus | MEDIUM |
| `byroredux/src/asset_provider/` | parsers; per-game | MEDIUM |
| `byroredux/src/scene` | per-game | MEDIUM |
| `byroredux/src/cornell.rs` | renderer Dim 12 | LOW |
| `byroredux/src/bench` | performance, runtime | LOW |
| `byroredux/src/debug_load.rs` | tooling | LOW |
| `byroredux/src/sf_smoke.rs` | starfield | LOW |
| `byroredux/src/game_profiles.rs` | tooling; per-game | MEDIUM |
| `byroredux/src/cli_args.rs` | tooling | MEDIUM |
| `byroredux/src/scene_import_cache.rs` | performance, nifal | MEDIUM |
| `byroredux/src/parsed_nif_cache.rs` | performance, nif | MEDIUM |
| `byroredux/src/anim_convert.rs` | nifal Dim 7, ecs Dim 9 | MEDIUM |
| `byroredux/src/workspace_hygiene_tests.rs` | tech-debt | LOW |
| `byroredux/src/app_events.rs` | ui, ecs | MEDIUM |
| `byroredux/src/app_frame.rs` | renderer, performance, concurrency | HIGH |
| `byroredux/src/app_step.rs` | save, performance, exterior | MEDIUM |
| `byroredux/src/components.rs` | ecs | MEDIUM |
| `byroredux/src/helpers.rs` | ecs | LOW |
| `byroredux/src/main.rs` | ecs, tooling | MEDIUM |
| `byroredux/src/` | ecs | MEDIUM |
| `byroredux/tests/` | regression, runtime | LOW |
| `scripts/` | runtime, performance | LOW |
| `docs/smoke-tests/` | runtime | LOW |
| `docs/` | tech-debt (doc rot) | LOW |
| `.Codex/commands/` | tech-debt (audit infrastructure) | LOW |
| `.zcode/` | tech-debt (audit infrastructure mirror) | LOW |
| `.Codex/issues/` | regression | LOW |
| `Cargo.` | tech-debt, safety (dependency drift) | LOW |
| `byroredux/Cargo.toml` | tech-debt | LOW |
| `README.md` | tech-debt (doc rot) | LOW |
| `ROADMAP.md` | tech-debt (doc rot) | LOW |
| `HISTORY.md` | tech-debt (doc rot) | LOW |
| `AGENTS.md` | tech-debt (doc rot) | LOW |
| `AGENTS.md` | tech-debt (doc rot) | LOW |
