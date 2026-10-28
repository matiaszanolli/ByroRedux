# SPT-2026-10-27-D4-01: Loose and console routes admit a `.spt` placeholder (and every non-actor loose NIF) at the `Actor` geometry tier, while the cell route admits it at `Prop`

Labels: low, bug, speedtree, renderer, performance
Source: docs/audits/AUDIT_SPEEDTREE_2026-10-27.md

**Report**: `docs/audits/AUDIT_SPEEDTREE_2026-10-27.md` · **Finding**: SPT-2026-10-27-D4-01 · **Severity**: LOW · **Dimension**: Per-Game Variants & Route Divergence
**Location**: `byroredux/src/scene/nif_loader.rs:1226-1237` (`spawn_nif_mesh`). Shared callers: `nif_loader.rs:138`, `:158` (`load_nif_from_args`: `--mesh` / `--tree`), `byroredux/src/debug_load.rs:150`, `byroredux/src/loading_screen.rs:347`, `byroredux/src/studio_host.rs:669`. The cell route is `byroredux/src/cell_loader/spawn/mesh_instance.rs:861`, `:907`.
**Introduced by**: `22d6e1a80` (2026-10-27)

## Description
`22d6e1a80` wraps the one `upload_scene_mesh` call in `spawn_nif_mesh` in `with_admission_class(GeometryAdmissionClass::Actor, ...)`, under the comment "Actors (the player and NPC bodies) are their own admission tier". But `spawn_nif_mesh` is not actor-only: it is the spawn tail of `load_nif_with_skeleton_from`, which every `load_nif_bytes` caller reaches - the `--tree` / `--mesh` visualiser, the console `debug_load` path (any archive path, `.spt` included), loading-screen meshes and Studio-host loads. So one SpeedTree placeholder quad is admitted at `Prop` when a TREE REFR spawns it (cell route) and at `Actor` when the loose or console route spawns it. The same applies to every non-actor NIF on those routes.

## Evidence
```rust
// nif_loader.rs:1229 - inside spawn_nif_mesh, used by every load_nif_bytes caller
// Actors (the player and NPC bodies) are their own admission tier:
// above props, below the ground, ...
let handle = match ctx.mesh_registry.with_admission_class(
    byroredux_renderer::GeometryAdmissionClass::Actor,
```
`crates/renderer/src/mesh.rs:55-63`: `Actor` = "the cap less the ground's reserve"; `Prop` = "the cap less the ground's and the actors' reserves".

## Impact
Small. A `--tree` / `--mesh` boot uploads into an empty pool, where the tiers make no difference. Once the pool is past the `Prop` ceiling (a large exterior radius), a console-loaded or loading-screen NIF still lands by eating the actor reserve the tiers keep for people, while cell-streamed props are refused. The comment also misdescribes the function, which led AUDIT_PERFORMANCE_2026-10-27 to sign the tiers off as correct.

## Related
`22d6e1a80`, `ab9d46d18` (geometry admission tiers); AUDIT_PERFORMANCE_2026-10-27 "Checked, no finding: Geometry admission tiers". Tier-policy owner: `/audit-renderer` / `/audit-performance`.

## Suggested Fix
Pass the admission class into `load_nif_with_skeleton_from` / `spawn_nif_mesh`: the `npc_spawn` callers use `Actor`, `load_nif_bytes` (visualiser, `debug_load`, loading screen, Studio) uses `Prop`. Correct the comment. A `.spt` placeholder should get the same tier on both routes.

## Completeness Checks
- [ ] **SIBLING**: Every `load_nif_with_skeleton_from` / `load_nif_bytes` caller classified (actor vs non-actor)
- [ ] **TESTS**: A regression test pins the admission class per caller (npc_spawn = Actor, loose/console = Prop)

