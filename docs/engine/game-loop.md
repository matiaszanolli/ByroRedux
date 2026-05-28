# Game Loop

The game loop lives in the binary crate at [`byroredux/src/main.rs`](../../byroredux/src/main.rs).
It uses winit's `ApplicationHandler` trait for event-driven execution. CLI
arguments choose between several scene-loading entry points; the default
is the spinning-cube demo. Once a scene is loaded, the per-frame tick runs
the same ECS schedule regardless of how the scene was created.

> **Currency note.** Reconciled 2026-05-28 against the current tree
> (the `App`/scheduler wiring shifted substantially after the
> 2026-04-07 version of this doc: `systems.rs` was split into a
> `systems/` submodule tree in Session 34, the M27 declared-access
> scheduler landed, the render path moved off `RedrawRequested` into
> `about_to_wait` (Phase 14), and an egui debug-UI overlay + metrics
> sampling were added). Function/struct/path names below are checked
> against `main.rs`, `byroredux/src/systems/`, and
> [`crates/core/src/ecs/scheduler.rs`](../../crates/core/src/ecs/scheduler.rs).

## CLI entry points

CLI args are parsed inline in [`byroredux/src/main.rs`](../../byroredux/src/main.rs)
(scanning `effective_args()`, not a derive-macro `Args` type) and in
[`byroredux/src/scene.rs`](../../byroredux/src/scene.rs)'s `setup_scene`.
The thin parser helpers (`parse_string_arg`, `parse_vec3_arg`,
`effective_args`, `set_effective_args`) live in
[`byroredux/src/cli_args.rs`](../../byroredux/src/cli_args.rs).

| Args | Scene loaded |
|---|---|
| (none) | Spinning cube demo |
| `path/to/mesh.nif` | Loose NIF file |
| `path/to/mesh.nif --kf path/to/anim.kf` | NIF + KF animation playback |
| `--bsa path.bsa --mesh meshes\foo.nif` | Single NIF extracted from a BSA |
| `--bsa meshes.bsa --mesh meshes\foo.nif --textures-bsa textures.bsa` | + textures |
| `--bsa meshes.bsa --tree trees\joshua01.spt --textures-bsa textures.bsa` | SpeedTree `.spt` placeholder billboard |
| `--esm FalloutNV.esm --cell CellID --bsa Meshes.bsa --textures-bsa Textures.bsa` | Interior cell from an ESM |
| `--master Skyrim.esm --esm Dawnguard.esm --cell ... --bsa ...` | DLC interior (repeatable `--master`) |
| `--esm FalloutNV.esm --grid 0,0 --radius 3 --bsa ...` | Exterior grid (radius 1..=7, default 3) |
| `--swf path/to/menu.swf` | Scaleform SWF menu overlay |
| `--game <key>` | Profile-driven archive/ESM expansion (Phase 20) |

Selected modifier / diagnostic flags (all read from `effective_args()`):

| Flag | Effect |
|---|---|
| `--debug` | Force `RUST_LOG=debug` + per-frame stats in the window title |
| `--cmd "console command"` | Headless: build an empty `World`, run one console command, exit (rejects cell/asset flags — #637) |
| `--bench-frames N` | Run N frames, print one `bench:` summary line, exit |
| `--bench-hold` | Keep running after the bench summary so `byro-dbg` can attach |
| `--screenshot PATH` | Capture a PNG on the bench-exit frame (requires `--bench-frames`) |
| `--camera-pos x,y,z` / `--camera-forward x,y,z` | Override the auto-framed initial camera pose |
| `--fly` / `--player` | Start in fly-camera vs kinematic-character mode (M28.5) |
| `--sounds-bsa PATH` | Opportunistically load the default footstep WAV (M44) |
| `--materials-ba2 PATH` | BGSM/BGEM material archive (FO4+; usually synthesized by `--game`) |
| `--sf-smoke <CELL_EDID>` | Headless Starfield ESM resolve-rate smoke test (requires `--esm`) |
| `--rotation-mode 0..=3` | Diagnostic switch for the REFR Euler→Y-up conversion |
| `--games-root PATH` | Base directory for `--game` profile expansion |

`--game <key>` is expanded **before** anything else reads argv:
`expand_game_profile_args` rewrites it into the underlying
`--esm` / `--bsa` / `--textures-bsa` / `--materials-ba2` set. The
expanded list is then frozen into a process-wide
`effective_args()` singleton (`cli_args::set_effective_args`) so every
downstream reader (scene setup, the NIF loader, debug-server loads,
cell-transition rebuilds) sees the post-expansion flags instead of
re-reading raw `std::env::args()`.

Each entry point lives in a small module
([`scene.rs`](../../byroredux/src/scene.rs) / its
[`scene/`](../../byroredux/src/scene) submodules,
[`cell_loader.rs`](../../byroredux/src/cell_loader.rs) / its
[`cell_loader/`](../../byroredux/src/cell_loader) submodules) that builds
the ECS world. Note that GPU-touching scene setup (`setup_scene`) needs the
renderer, so it runs in `resumed` — not at startup — once `VulkanContext`
exists.

## Startup Sequence

```
1. expand_game_profile_args(argv)  → effective_args() singleton seeded
2. env_logger from RUST_LOG (--debug forces "debug"); init_tracing()
3. Verify C++ bridge (byroredux_cxx_bridge::ffi::native_hello())
4. Detect early-exit flags (after logging/tracing/bridge-verify):
     --sf-smoke   → sf_smoke::run(), return
     --cmd        → headless command run (rejects cell/asset flags), return
5. App::new(debug_mode, &args):
     - Create World, insert the built-in resources (see below)
     - Register scripting + physics component storages
     - Build the ScriptRegistry (papyrus_demo spawners — M47.0)
     - Build the Scheduler (M27 declared-access schedule — see below)
     - Snapshot SystemList + SchedulerAccessReport into the World
     - Build the console CommandRegistry resource
     - Start the debug server (feature "debug-server", port 9876 / BYRO_DEBUG_PORT)
6. Stash bench/screenshot/camera-override args onto the App
7. event_loop.run_app(&mut app)  → enters winit's ApplicationHandler
```

`App::new` inserts a large built-in resource set into the `World`,
including (non-exhaustive): `DeltaTime(0.0)`, `TotalTime(0.0)`,
`EngineConfig`, `DebugStats`, `ScratchTelemetry`, `SkinCoverageStats`,
`CpuFrameTimings`, `SchedulerSystemTimings`, `MetricsState` +
`MetricsSnapshot` (debug-UI sampling), `PendingDebugLoadSlot`,
`SelectedRef`, `InputState`, `StringPool`, `FormIdPool`,
`AnimationClipRegistry`, `NameIndex`, `SubtreeCache`, `CellRootIndex`,
`PhysicsWorld`, `ContactConfig`, `AudioWorld`, `FootstepConfig` +
`FootstepScratch`, the parsed-NIF / scene-import caches, the
`game_profiles` registry, and the `ScriptRegistry`. The `Scheduler`,
`World`, and console `CommandRegistry` are all constructed here, before
the event loop and before the renderer exists.

## The system schedule (M27 declared-access)

Systems are grouped into **stages** that run sequentially in a fixed
order, defined by the `Stage` enum in
[`crates/core/src/ecs/scheduler.rs`](../../crates/core/src/ecs/scheduler.rs):

```
Early = 0  →  Update = 1  →  PostUpdate = 2  →  Physics = 3  →  Late = 4
```

Within a stage, non-exclusive systems run in **parallel** (rayon), and
**exclusive** systems (`add_exclusive`) run alone *after* the stage's
parallel batch. Parallel systems can declare their data access via
`add_to_with_access(stage, system, Access::new()...)`; the scheduler's
analyzer pairs declared systems and reports any read/write or write/write
conflicts (surfaced to the operator via the `sys.accesses` console
command, snapshotted into the `SchedulerAccessReport` resource at build
time). M27 Phase 3 reached **0 unknown / 0 conflicts** by either
declaring access on parallel systems or demoting genuinely
serial-but-disjoint systems (e.g. `spin_system`, `animate_lights_system`,
`audio_system`) to exclusive.

The live schedule built in `App::new` is:

| Stage | System | Mode | Notes |
|---|---|---|---|
| `Early` | `player_controller_system` | declared | Dispatches to `fly_camera_system` **or** `character_controller_system` by `PlayerMode`; access is the union of both inner systems (M28.5 + M27 Phase 3) |
| `Early` | `weather_system` | declared | Weather/TOD/sky/cloud sim (M33) |
| `Early` | `timer_tick_system` | declared | Advances `ScriptTimer`, fires `TimerExpired` |
| `Update` | papyrus-demo dispatchers (`rumble_*`, `quest_advance_*`, `dlc2_ttr4a_*`, `mg07_*`) | exclusive | M47.0 event-driven script demos; early-return when no event present |
| `Update` | `animation_system` | declared | Advances `AnimationPlayer`/`AnimationStack`, writes `Transform` + all animated-channel storages |
| `Update` | `spin_system` | exclusive | Demo cube spin |
| `Update` | `animate_lights_system` | exclusive | Procedural candle/chandelier flicker (Phase 17) |
| `PostUpdate` | `make_transform_propagation_system()` | declared | Parent→child `GlobalTransform` (BFS) |
| `PostUpdate` | `footstep_system` | exclusive | Reads propagated `GlobalTransform` (#848) |
| `PostUpdate` | `particle_system` | exclusive | Needs final emitter world origin (#401) |
| `PostUpdate` | `billboard_system` | exclusive | Overwrites computed world rotation (#225) |
| `PostUpdate` | `make_world_bound_propagation_system()` | exclusive | Runs last so it sees billboard rotations (#217) |
| `PostUpdate` | `submersion_system` | exclusive | Camera-vs-water test → `SubmersionState` |
| `Physics` | `physics_sync_system` | declared | Rapier step → `Transform` writeback |
| `Late` | `camera_follow_system` | declared | M28.5 — runs after physics settle |
| `Late` | `reverb_zone_system` | declared | Cell-acoustics → audio reverb send (M44 Phase 6) |
| `Late` | `audio_system` | exclusive | Listener pose / emitter update (M44) |
| `Late` | `log_stats_system` | declared | Periodic stats log |
| `Late` | `metrics_sample_system` | declared | ~2 Hz CPU/RAM/VRAM/GPU snapshot for the debug UI |
| `Late` | `event_cleanup_system` | exclusive | Drops transient marker components |

The debug server (feature-gated) may register additional systems via
`byroredux_debug_server::start(&mut scheduler, port)`.

The schedule is identical whether the scene is a single sweetroll or a
full FNV interior — it isn't customised per entry point. Systems whose
work is scene-dependent (animation with no clips, streaming, footsteps
with no emitters) early-return cheaply.

There is **no standalone `input_system`**: raw input is captured directly
in `window_event` / `device_event` into the `InputState` resource, which
`fly_camera_system` (inside `player_controller_system`) consumes the next
frame.

## Event Flow

The `App` struct implements `winit::application::ApplicationHandler`, so
each event lands on a method.

### `resumed` (once, on first window creation)

```
1. Create the winit Window (via byroredux_platform::window::create_window)
2. Get raw display + window handles
3. VulkanContext::new(display, window_handle, [w, h])  (full init chain — see renderer.md)
4. Insert renderer-derived resources: ScreenshotBridge, AllocatorResource,
   GpuMemoryBudget
5. ctx.init_egui(...) + DebugUiState::new(...)  (debug-UI overlay)
6. setup_scene()  ← cell loader / NIF demo / SWF menu hook in here
7. scheduler.run(&world, 0.0)  ← prime transform/bound state before frame 0
8. last_frame = Instant::now()
```

Vulkan needs the window handles, so context creation has to wait until
`resumed`. Mesh and texture uploads also need the device, so any "load
this NIF and upload it to the GPU" work runs through `setup_scene` here,
not at startup. The ECS world itself is built in `App::new` before the
renderer exists, so non-GPU state is ready immediately. A single
`scheduler.run(&world, 0.0)` is fired at the end of `resumed` to prime
transform/bound propagation before the first rendered frame
(M41.0 Phase 1b.x).

### `about_to_wait` (every frame — the per-frame tick)

This is the heart of the game loop. Since Phase 14, **rendering is driven
from here**, not from `RedrawRequested` (on Wayland + winit 0.30 the
`request_redraw()` → compositor frame-callback → `RedrawRequested`
round-trip gated the loop at the compositor's pace; driving the draw from
`about_to_wait` uncaps it while MAILBOX present mode still vsyncs the
actual presentation).

```
1. now = Instant::now(); dt = now - last_frame; last_frame = now
     (BYROREDUX_FIXED_DT env var overrides dt for golden-frame tests)
2. world_resource_set::<DeltaTime>  / ::<TotalTime>   (interior mutability)
3. Refresh DebugStats (frame time, entity count, meshes/textures in use,
     registry counts, SkinSlotPool telemetry), ScratchTelemetry,
     SkinCoverageStats
4. scheduler.run(&world, dt)        ← all ECS systems execute here
5. step_streaming()                 ← M40 exterior cell stream (no-op otherwise)
6. step_debug_loads()               ← drain debug-UI / debug-server load queue
7. step_cell_transition()           ← drain door.teleport interior↔exterior swap
8. Update the window title (~4×/sec) if EngineConfig.debug_logging
9. render_one_frame(event_loop)     ← build render data + draw + present
10. --bench-frames: on the target frame, print the bench summary
      (and screenshot if requested), then exit unless --bench-hold
```

### `render_one_frame` (called from `about_to_wait`, Phase 14)

This is the former `RedrawRequested` body, pulled out as an inherent
method on `App` and bracketed into three phases (Phase 15) for the
debug-UI Metrics panel:

```
1. Build the debug-UI snapshot + run egui → PanelOutputs; apply outputs
2. ctx.submit_egui_frame(...) if the overlay produced a frame
3. build_render_data(&world, ...) — collect draw + water commands, lights,
     bone-world matrices, skin offsets, material table
4. Mirror material-table telemetry into ScratchTelemetry
5. Rebuild the geometry SSBO if dirty
6. Tick + render the UI overlay (Ruffle SWF player), upload its texture
7. ctx.draw_frame(...) — record + submit + present; capture FrameTimings
8. Feed CpuFrameTimings back into the World (fence wait / submit-present /
     between-frames)
```

`build_render_data` lives in [`byroredux/src/render/`](../../byroredux/src/render)
(the `render.rs` file was split into a `render/` module tree:
`static_meshes.rs`, `skinned.rs`, `particles.rs`, `lights.rs`, `water.rs`,
`sky.rs`, `camera.rs`, plus `mod.rs`). It walks the ECS once per frame to
collect visible meshes, their `MeshHandle`s, world transforms, materials,
lights, decal flags, water planes, and skinned-mesh bone data. The output
is plain owned data (`Vec<DrawCommand>`, `Vec<WaterDrawCommand>`,
`Vec<GpuLight>`, the bone-world matrix scratch, the `MaterialTable`), so
the renderer can chew on it without holding any ECS locks during command
recording. Those scratch buffers are owned on `App` and reused frame to
frame so their allocations persist (#243 / #253 / #509).

### `WindowEvent::RedrawRequested`

```
(empty — Phase 14)
```

The OS still fires this on window expose / resize / first paint, but the
body is intentionally bare: the next `about_to_wait` tick does the render
work. The arm is kept (not removed) only to keep the match exhaustive.

### `WindowEvent::Resized`

```
1. ctx.recreate_swapchain([w, h])
2. Update the active camera's aspect ratio
```

See [Vulkan Renderer — Resize](renderer.md#resize) for the atomic-handoff
details.

### `WindowEvent::CloseRequested`

```
1. Unload every streamed cell (cell_loader::unload_cell) + flush_pending_destroys
     (M40 — releases per-cell BLAS/mesh/texture refs before ctx teardown)
2. streaming.shutdown(1s timeout)  — joins the worker thread cleanly
3. Drop the VulkanContext (waits for device idle, tears down in reverse)
4. Drop the window
5. event_loop.exit()
```

The streaming-shutdown sweep is load-bearing: every streamed cell owns GPU
resources released only via `unload_cell`, and skipping it makes the
allocator find dangling refs at context destruction and SIGSEGV as the
orphaned device handles get reaped (#732 / #856).

### Input events

`KeyboardInput` arrives via `window_event`; `MouseMotion` arrives via
`device_event`. Both are forwarded into the `InputState` resource, which
`fly_camera_system` (gated behind `player_controller_system`) consumes on
the next frame. Mouse motion is only applied while the cursor is grabbed.
Notable keybinds handled in `window_event`:

- **Escape** — toggle mouse capture (cursor grab + hide).
- **F** — toggle Walk ↔ Fly player mode (`toggle_player_mode`, M28.5).
- **F3** — toggle the egui debug overlay (`DebugUiState::toggle`).

When the debug overlay is visible and egui consumes an event (e.g. a click
inside an egui window), the rest of the input dispatch is skipped so the
fly camera doesn't fight an egui slider drag — except `CloseRequested` /
`Resized`, which always run.

## Resource update pattern

`about_to_wait` only has `&mut self` on `App`, but `World` queries use
`&self`. The two-line helper in
[`byroredux/src/helpers.rs`](../../byroredux/src/helpers.rs) threads the
mutation through interior mutability:

```rust
pub(crate) fn world_resource_set<R: Resource>(world: &World, f: impl FnOnce(&mut R)) {
    let mut guard = world.resource_mut::<R>();
    f(&mut guard);
}

// Usage:
world_resource_set::<DeltaTime>(&self.world, |r| r.0 = dt);
world_resource_set::<TotalTime>(&self.world, |r| r.0 += dt);
```

## App Struct

The `App` struct has grown well past the four-field sketch the earlier
version of this doc carried. The structurally important fields:

```rust
struct App {
    window: Option<Window>,
    renderer: Option<VulkanContext>,
    world: World,
    scheduler: Scheduler,
    last_frame: Instant,
    ui_manager: Option<UiManager>,          // Ruffle SWF overlay
    ui_texture_handle: Option<u32>,
    draw_commands: Vec<DrawCommand>,         // reused per-frame scratch
    water_commands: Vec<WaterDrawCommand>,   // reused per-frame scratch
    gpu_lights: Vec<GpuLight>,               // reused per-frame scratch
    bone_world: Vec<[[f32; 4]; 4]>,          // M29.5/M29.6 skinning palette
    skin_slot_pool: SkinSlotPool,            // persistent per-entity bone slots
    skin_offsets: HashMap<EntityId, u32>,
    material_table: MaterialTable,           // R1 deduplicated material SSBO
    streaming: Option<WorldStreamingState>,  // M40 exterior cell streaming
    // bench_* fields: --bench-frames / --bench-hold / --screenshot plumbing
    // camera_pos_override / camera_forward_override
    #[cfg(feature = "debug-server")] debug_server: Option<DebugServerHandle>,
    debug_ui: Option<DebugUiState>,          // egui overlay state
    debug_ui_refresh_entities: bool,
    last_redraw_end: Option<Instant>,        // between-frames timing
    // ... bench accumulators, screenshot deadline counters, etc.
}
```

`window` and `renderer` are `Option` because they're created in `resumed`
(winit requires the event loop to be running before window creation).
`world`, `scheduler`, and the scratch buffers are constructed in
`App::new` before the event loop starts. Scene setup is no longer a
deferred closure stored on `App` — `resumed` calls `App::setup_scene`,
which delegates to `scene::setup_scene(&mut world, ctx, ...)` once the
renderer is up; that's where the cell loader, the NIF demo, and the SWF
menu loader all hook in.

The per-frame work split across helper methods on `App`:

- `render_one_frame` — build render data, run egui, draw + present.
- `step_streaming` — M40 exterior cell streaming: drain worker payloads,
  diff the loaded cell set against the player's grid, dispatch load/unload.
- `step_debug_loads` — drain the `PendingDebugLoadSlot` populated by the
  debug-server's `LoadNif` / `LoadInteriorCell` / `LoadExteriorCell`
  handlers.
- `step_cell_transition` — drain a queued `PendingCellTransition` (posted
  by the `door.teleport` console command) and run the interior↔exterior
  swap orchestrator (M40 Phase 2 Stage 3).

## Per-frame timing reference

Bench-of-record numbers (RTX 4070 Ti, Ryzen 9 7950X), from the
R6a-stale-13 refresh at commit `4e2ebe8c` (2026-05-28). See
[ROADMAP.md](../../ROADMAP.md) for the authoritative, continuously
refreshed bench table:

| Cell | Entities | FPS | Frame | Fence | Draws |
|---|---|---|---|---|---|
| FNV Prospector Saloon (interior) | 3507 | 71.4 | 14.00 ms | 11.65 ms | 1225 |
| Skyrim SE Whiterun Bannered Mare (interior) | 3211 | 329.8 | 3.03 ms | 1.01 ms | 1296 |
| FO4 MedTekResearch01 (interior) | 15546 | 90.7 | 11.02 ms | 4.73 ms | 8304 |

Whiterun is the control bench (Skyrim ships real `bhk` collision, so its
entity count is stable); it rose +14.6% FPS over the 125-commit window vs
the prior `a9bbe8d1` record, confirming the steady-state hot path did not
regress. FNV and FO4 grew their entity counts ~37–42% after #1294 began
synthesizing static-trimesh colliders for architecture that has no
authored `bhk` collision — each synthesized collider adds RT BLAS
geometry, which drove Prospector's `fence` super-linearly (2.62 → 11.65 ms)
and its FPS down 56%. That collider cost is tracked as a follow-up
(R6a-stale-13-collider-cost). FO4's CPU side actually improved (`brd`
7.81 → 2.63 ms), so MedTek is now GPU-bound rather than CPU-bound.

The CPU game-loop work itself (scheduler.run + build_render_data) stays
well under the GPU path. The debug-UI Metrics panel breaks the per-frame
wall time into `atw_pre` / `atw_scheduler` / `atw_post` and the
`render_one_frame` pre-draw / draw-call / post-draw brackets, with
per-system scheduler timings (`SchedulerSystemTimings`) and CPU frame
timings (`CpuFrameTimings`, including the GPU fence wait) for finding which
phase dominates a given frame.
