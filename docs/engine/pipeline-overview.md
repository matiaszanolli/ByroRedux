# Pipeline Overview: From CLI to Pixel

Every other doc in this directory covers one subsystem in isolation — the
NIF parser, the ECS, the renderer, NIFAL. None of them trace a single
request all the way through. This doc is the connective tissue: one
narrative following an interior cell load from the command line to a
presented frame, citing the real entry-point function at each handoff and
linking out to the subsystem doc that covers it in depth. It intentionally
does **not** re-explain those subsystems — read this first for the shape of
the pipeline, then drill into the linked doc for the stage you care about.

> **Currency note.** Verified against the tree as of 2026-07-15 (source
> citations below, not reconstructed from other docs — several, notably
> [Game Loop](game-loop.md), currently describe an older `main.rs`-centric
> shape and will need their own reconciliation pass; that's tracked as
> follow-up, not fixed here).

**Scope**: the interior-cell-load path (`--esm X.esm --cell Y --bsa Z`),
the most uniformly supported path across all seven games. Exterior-grid
streaming, loose-NIF loading, and the Cornell test harness share most of
stages 4 onward but branch earlier — noted inline where relevant.

## 1. CLI entry → scene dispatch

`fn main()` (`byroredux/src/main.rs:65`) is a one-line shell that calls
`boot::run()`. Despite what [Game Loop](game-loop.md) currently says, the
real CLI/boot logic doesn't live in `main.rs` — `pub(crate) fn run()`
(`byroredux/src/boot.rs:65`) builds the `winit::EventLoop`, constructs the
`App`, and calls `event_loop.run_app(&mut app)`.

`App` implements `winit::ApplicationHandler` (`main.rs:722`).
`resumed()` (`main.rs:723`) creates the window + `VulkanContext`, then
calls `self.setup_scene()` (`main.rs:806`), forwarding to
`scene::setup_scene` (`byroredux/src/scene.rs:75`). That function is the
actual dispatch point: `--cornell` → the Cornell-box RT reference scene;
`--esm` → interior (`--cell`, `scene.rs:196-220`) or exterior
(`--grid`, `scene.rs:228`); otherwise falls through to loose-NIF loading
(`load_nif_from_args`, `byroredux/src/scene/nif_loader.rs`).

## 2. ESM cell lookup

Interior loading enters at `cell_loader::load_cell_with_masters`
(`byroredux/src/cell_loader/load.rs:190`), called from `scene.rs:200`.

It parses the ESM(s) in load order via `parse_record_indexes_in_load_order`
(`byroredux/src/cell_loader/load_order.rs:129`), which reads each plugin's
TES4 header then calls `esm::records::parse_esm_with_load_order`
(`crates/plugin/src/esm/records/mod.rs:122`). That function dispatches
top-level GRUPs; on `b"CELL"` it calls the CELL-walker entry point,
`parse_cell_group` (`crates/plugin/src/esm/cell/walkers.rs:282`), which
populates `EsmIndex.cells`. Back in `load_cell_with_masters`, the
requested editor ID is looked up in that index (`load.rs:223`).

See [ESM Records](esm-records.md) and [Cell Record Structure](../legacy/gamebryo-2.3-architecture.md)
for the record layout this walk produces.

## 3. BSA/BA2 → NIF bytes

Placed references name mesh paths, not bytes. `TextureProvider`
(`byroredux/src/asset_provider/texture.rs` — despite the name, it owns
both texture and mesh archives) resolves them: `extract_mesh()`
(`texture.rs:49`) normalizes the path and loops the mesh archives calling
`Archive::extract()` (`byroredux/src/asset_provider/archive.rs:31`), which
dispatches to `byroredux_bsa::BsaArchive` or `Ba2Archive` depending on
which variant `Archive::open()` auto-detected from the file magic.
`TextureProvider` also implements `byroredux_nif::import::MeshResolver`
(`texture.rs:60-64`) — the trait object the NIF importer uses for
on-demand external-mesh lookups (attach points, `.egm` morphs).

See [Archives](archives.md) for the BSA/BA2 format details.

## 4. NIF parse

`byroredux_nif::parse_nif` (`crates/nif/src/lib.rs:197`) wraps
`parse_nif_with_options`, which runs exactly three phases:
`parse_header()` (header decode + endianness validation),
`dispatch_blocks()` (the ~254-arm block-type dispatch table in
`crates/nif/src/blocks/mod.rs`), and `finalize_scene()` (post-link scene
assembly, folding truncation/recovery telemetry into the returned
`NifScene`). See [NIF Parser](nif-parser.md).

## 5. NIF → `Imported*` → NIFAL `translate()`

The cell-load caller is `cell_loader::references::import::parse_and_import_nif`
(`byroredux/src/cell_loader/references/import.rs:43`), which: calls
`parse_nif`; does BSXFlags editor-marker filtering (game-era-gated); then
calls `byroredux_nif::import::import_nif_with_collision_and_resolver`
(`crates/nif/src/import/mod.rs:528`), returning raw `Imported*` structs
(`ImportedMesh`, `ImportedCollision`), plus separate calls for lights,
particle emitters, and embedded animations.

Note this is a **different** entry point than `import_nif_scene` /
`import_nif_scene_with_resolver` (`crates/nif/src/import/mod.rs:104,115`)
— those build a hierarchical `ImportedScene` and are used by the
loose-NIF path only. The cell-load path uses the flat variant.

Materials cross the canonical boundary in
`byroredux::material_translate::translate_material`
(`byroredux/src/material_translate.rs:73`) — this matches
[NIFAL](nifal.md)'s claim that materials are the converged reference
slice. `merge_bgsm_into_mesh` (`import.rs:113`) runs first, folding in
FO4+ external BGSM/BGEM data. See [NIFAL](nifal.md) and
[Material Abstraction](material-abstraction.md).

## 6. Cell loading → ECS spawn

Back in `load_cell_with_masters`: FO4+ precombine spawn
(`precombined::spawn_precombined_meshes`, `load.rs:297`), then
`load_references(...)` (`load.rs:327`, in `cell_loader/references/mod.rs`)
walks placed REFRs/ACHRs, expands PKIN/SCOL containers, calls
`parse_and_import_nif` (cached per-NIF), and dispatches to
`spawn_placed_instances` (`byroredux/src/cell_loader/spawn.rs:180`) — the
function that actually calls `world.spawn()` and inserts components:
mesh entities (`spawn.rs:841` → `Transform`/`GlobalTransform`/`MeshHandle`),
light entities (`spawn.rs:364`), particle emitters (`spawn.rs:458`),
collision entities. Every entity spawned in the load gets `CellRoot`
stamped (`load.rs:99-114`) so cell unload can find and despawn them.

See [ECS](ecs.md) for the component/storage model this populates into.

## 7. Per-frame scheduler tick

The `Scheduler` is built once in `boot::run()` (`boot.rs:436`) with
`Stage::Early/Update/PostUpdate/Late` registrations — richer than a flat
"camera, animation, transform, spin, stats" list:

- **Early**: `player_controller_system` (fly-cam vs. kinematic character
  controller), `weather_system`, `timer_tick_system`.
- **Update**: scripting/quest dispatchers, `make_animation_system()`,
  `spin_system`, `animate_lights_system`.
- **PostUpdate**: `make_transform_propagation_system()`, footstep,
  particle, billboard, world-bound-propagation, submersion systems.
- **Late**: ragdoll writeback, audio, `log_stats_system`, scripting event
  cleanup.

Driven from `App::about_to_wait` (`main.rs:1035`), which calls
`self.scheduler.run(&self.world, dt)` (`main.rs:1147`) before rendering
the frame. (A priming zero-dt run also fires once at the end of
`resumed()`.) See [ECS](ecs.md) for the scheduler/stage model itself and
[Game Loop](game-loop.md) for the winit event wiring — note its
`main.rs`-centric framing predates the `boot.rs` split and is due a
refresh.

## 8. Render data collection

`App::render_one_frame` (`main.rs:378`) calls `build_render_data(...)`
(`byroredux/src/render/mod.rs:309`), which queries the ECS `World`
(meshes, lights, skinning, particles, water, camera, sky) and fills
scratch buffers persisted on `App` — `draw_commands`, `gpu_lights`,
`bone_world`, `material_table`, etc. — to avoid per-frame reallocation.
It returns a `RenderFrameView` (camera matrices, sky params, DOF params).
`render_one_frame` hands that plus the scratch buffers into
`ctx.draw_frame(FrameInputs { .. })` (`main.rs:584`).

## 9. Scene buffer upload

`crates/renderer/src/vulkan/scene_buffer/upload.rs` exposes one method
per GPU resource category — `upload_lights`, `upload_camera`,
`upload_dalc`, `upload_bone_worlds`, `upload_pending_bind_inverses`,
`upload_instances`, `upload_materials`, `upload_indirect_draws`,
`upload_terrain_tiles`. All are called from inside `draw_frame` itself
(not from `main.rs`), before the geometry pass records. See
[Shader Pipeline](shader-pipeline.md) for the GPU-side struct layouts
these calls feed.

## 10. Vulkan `draw_frame` → present

`VulkanContext::draw_frame` (`crates/renderer/src/vulkan/context/draw.rs:2182`):
fence wait → `acquire_next_image` → TAA Halton jitter computed → the
`upload_*` calls above → `record_geometry_pass(...)` (`draw.rs:943`,
called at `draw.rs:3764` — PBR + RT ray queries for shadows,
reflections, and 1-bounce GI) → `record_post_passes(...)` (`draw.rs:523`,
called at `draw.rs:3797`), which records, in fixed order per its own doc
comment: water-caustic barrier → SVGF temporal + à-trous denoise →
caustic splat → volumetrics → TAA resolve → SSAO → bloom → final
composite (ACES tone map) → `queue_submit` → `queue_present`.

This matches [Vulkan Renderer](renderer.md)'s `draw_frame` walkthrough and
[Shader Pipeline](shader-pipeline.md)'s pass table; neither currently
names the `record_geometry_pass`/`record_post_passes` split (extracted
under #1748) as an anchor — this doc is the first to cite those two
function names directly.

## Candidate follow-ups

This doc traces one flow. A few others would earn their own
cross-cutting narrative if the same "nothing connects the subsystem docs"
gap shows up there too — not written yet, listed here so the next pass
doesn't have to rediscover the need:

- **Save/load full round-trip** — ECS snapshot → validation gates →
  atomic write/ring buffer → load-apply (cell reload + FormId-keyed
  deltas + player-pose restore). Touches `crates/save`, `cell_loader`,
  and the M45.1 live-load-apply path.
- **NPC spawn → AI package execution** — `npc_spawn.rs` instantiation
  through package selection/priority stack to an actor executing a
  Sandbox procedure. Touches `plugin` (PACK records), `scripting`, `core`.
- **Exterior grid streaming** — the Phase 1/2 async pre-parse and
  interior↔exterior cell-swap path (`script.activate <door>`), which
  this doc's interior-only scope skips entirely.
