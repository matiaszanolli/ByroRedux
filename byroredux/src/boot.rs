//! Boot/config plumbing split out of `main.rs` (#1858 / TD1-003):
//! CLI arg parsing + expansion, `World`/`Scheduler` construction, and
//! the winit event-loop kickoff. `App`'s `ApplicationHandler` impl and
//! per-frame stepping stay in `main.rs` / `app_step.rs` respectively.

use anyhow::Result;
use byroredux_core::animation::AnimationClipRegistry;
use byroredux_core::console::CommandRegistry;
use byroredux_core::ecs::{
    Access, ActiveCamera, DebugStats, DeltaTime, EngineConfig, MetricsSnapshot, Scheduler,
    SkinCoverageStats, Stage, SystemList, TotalTime, Transform, World,
};
use byroredux_core::string::StringPool;
use winit::event_loop::{ControlFlow, EventLoop};

use crate::cli_args::{parse_string_arg, parse_vec3_arg};
use crate::commands::build_command_registry;
use crate::components::{CellRootIndex, FootstepConfig, InputState, NameIndex, SubtreeCache};
use crate::systems::{
    animate_lights_system, footstep_system, log_stats_system, make_animation_system,
    make_billboard_system, make_transform_propagation_system, make_world_bound_propagation_system,
    metrics_sample_system, particle_system, spin_system, weather_system, MetricsState,
};
use crate::App;

/// Install a `tracing` subscriber for the cell-load span ladder
/// (#886 / INFRA-PERF-01). Default builds register a no-op subscriber
/// — `#[tracing::instrument]` macros expand to function-pointer-check
/// stubs that drop the span data, so there's no measurable overhead
/// on the hot path. With `--features tracing-tracy`, spans are piped
/// into the Tracy profiler instead, giving wall-clock visibility for
/// findings like #877 (BSA mutex contention), #879 (REFR mesh upload
/// fence-waits), #880 (NPC NIF re-parse), #881 (texture upload
/// budget), #882 (StringPool intern), #883 (multi-walk unload).
///
/// `env_logger` stays the canonical log path; tracing here is
/// strictly for span-based wall-clock profiling, not logging
/// duplication.
fn init_tracing() {
    // Default-build path: no fmt layer, no Tracy — `instrument` macros
    // become noop spans. Even the no-op `set_global_default` is
    // cheap; we skip it entirely so default builds are byte-identical
    // pre-this-commit at the subscriber level.
    #[cfg(feature = "tracing-tracy")]
    {
        use tracing_subscriber::layer::SubscriberExt;
        use tracing_subscriber::util::SubscriberInitExt;

        let registry = tracing_subscriber::registry()
            .with(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
            )
            .with(tracing_tracy::TracyLayer::default());
        registry.init();
        log::info!(
            "tracing-tracy: spans → Tracy profiler (run `tracy-capture -o run.tracy` \
             before launching to capture)"
        );
    }
}

/// Entry point body, extracted verbatim from the former `fn main()`
/// (#1858 / TD1-003) so `main.rs` becomes a one-line dispatcher.
pub(crate) fn run() -> Result<()> {
    // Whole-run heap profiler (PERF-D2-NEW-03 / #1381). Held for the
    // lifetime of `run`; on drop (process exit) `dhat` writes
    // `dhat-heap.json` to the CWD. No-op unless built with
    // `--features dhat-heap`.
    #[cfg(feature = "dhat-heap")]
    let _dhat_profiler = dhat::Profiler::new_heap();

    // Phase 20 — expand `--game <key>` into the full set of
    // `--esm` / `--bsa` / `--textures-bsa` / `--materials-ba2`
    // args BEFORE anything else reads argv. User-supplied flags
    // appear first in the resulting Vec and win
    // first-occurrence-wins for unique flags like `--esm`;
    // additive flags (`--bsa` / `--textures-bsa`) get both the
    // user's archives AND the profile's defaults.
    //
    // Phase 20.1 — seed the expanded args into the process-wide
    // `effective_args()` singleton so downstream readers
    // (scene.rs, nif_loader, debug_load, transition rebuilds)
    // see the post-expansion list instead of re-reading raw
    // `std::env::args()` and losing the synthesized flags.
    let args: Vec<String> = expand_game_profile_args(std::env::args().collect());
    crate::cli_args::set_effective_args(args.clone());
    let debug_mode = args.iter().any(|a| a == "--debug");

    // --bench-frames N: run N frames, emit a single `bench:` summary
    // line to stdout, then exit. Intended for CI perf tracking and for
    // reproducing the ROADMAP's sweetroll FPS claim without interactive
    // window-title reading. See #366.
    let bench_frames = args
        .iter()
        .position(|a| a == "--bench-frames")
        .and_then(|i| args.get(i + 1).and_then(|v| v.parse::<u32>().ok()));

    // --screenshot PATH: when set, request a screenshot on the bench
    // exit frame (requires --bench-frames) and write to PATH before
    // quitting. No-op without --bench-frames. Used for offline
    // rendering diagnostics / visual regression baselines.
    let screenshot_path = parse_string_arg(&args, "--screenshot");

    // --bench-hold: after `--bench-frames N` emits its summary, keep
    // the engine running (rendering, debug server reachable) instead
    // of exiting. The bench summary still prints exactly once, at the
    // target frame. Used by audit / triage workflows that need
    // `byro-dbg` to connect post-bench and run console commands like
    // `tex.missing` / `tex.loaded` against the loaded scene — pre-flag
    // the binary exited too quickly for a TCP client to attach. No-op
    // without `--bench-frames`. See FNV-D5 audit (`docs/audits/
    // AUDIT_FNV_2026-05-08.md` § Coverage gaps).
    let bench_hold = args.iter().any(|a| a == "--bench-hold");

    // --camera-pos x,y,z + --camera-forward x,y,z — override the
    // auto-computed initial camera pose. Useful for capturing specific
    // framing in bench mode without needing interactive WASD input.
    // Both args are `,`-separated floats. Pass one or both; missing
    // forward defaults to `-Z` toward the origin.
    let camera_pos = parse_vec3_arg(&args, "--camera-pos");
    let camera_forward = parse_vec3_arg(&args, "--camera-forward");

    // --rotation-mode 0..=3 — diagnostic switch for the REFR
    // Euler→Y-up conversion. See `cell_loader::euler_zup_to_quat_yup_refr`
    // doc for what each mode means. Used to triage the "large statics
    // misplaced + 90° rotated" symptom by screenshotting each candidate
    // on a known-good cell. Defaults to 0 (current shipping behavior).
    if let Some(idx) = args.iter().position(|a| a == "--rotation-mode") {
        if let Some(mode) = args.get(idx + 1).and_then(|v| v.parse::<u8>().ok()) {
            crate::cell_loader::set_refr_rotation_mode_diag(mode.min(3));
            log::info!("--rotation-mode {} active", mode.min(3));
        }
    }

    // Set up logging. --debug forces debug level.
    if debug_mode {
        std::env::set_var(
            "RUST_LOG",
            std::env::var("RUST_LOG").unwrap_or("debug".into()),
        );
    }
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    init_tracing();

    log::info!("ByroRedux starting");
    log::info!("{}", byroredux_cxx_bridge::ffi::native_hello());

    // --sf-smoke <CELL_EDID>: Starfield ESM resolve-rate smoke test.
    // Headless planning-phase deliverable that walks the ESM under the
    // current `GameKind` dispatch, picks one named interior cell, and
    // reports the % of REFRs whose base_form_id resolves to a known
    // StaticObject. Gate question for ROADMAP Milestone B (Starfield
    // interior renders). See #763 / SF-D6-04. Requires `--esm <PATH>`.
    // Logger is the global one initialised at line 152 above; the smoke
    // is a no-window, no-engine path that prints to stdout and exits.
    if let Some(idx) = args.iter().position(|a| a == "--sf-smoke") {
        let cell_edid = args
            .get(idx + 1)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("--sf-smoke requires a cell EDID argument"))?;
        let esm_path = parse_string_arg(&args, "--esm").ok_or_else(|| {
            anyhow::anyhow!("--sf-smoke requires --esm <PATH> to specify the ESM")
        })?;
        return crate::sf_smoke::run(std::path::Path::new(&esm_path), &cell_edid);
    }

    // Headless --cmd mode: execute command and exit without creating a window.
    if let Some(cmd_idx) = args.iter().position(|a| a == "--cmd") {
        // #637 / FNV-D5-03 — the headless path builds an empty World
        // with only `DebugStats::default()` and the command registry, so
        // every cell-aware command (`stats`, `entities`, `tex.missing`,
        // `light.dump`, etc.) returns zeros regardless of which
        // `--esm` / `--bsa` flags were also passed. Reject the
        // combination with a clear error rather than silently producing
        // misleading output that a CI baseline check would otherwise
        // accept as valid.
        //
        // Audit Option (b) — wiring the cell loader into this path so
        // cell-aware stats work without a window — is the long-term
        // unblock for CI regression checks, but is substantially more
        // scope than this LOW-severity bundle covers. Filed as a
        // follow-up if/when CI starts asserting on baselines.
        let conflicting: Vec<&str> = [
            "--esm",
            "--bsa",
            "--textures-bsa",
            "--master",
            "--grid",
            "--cell",
            "--wrld",
            "--radius",
            "--mesh",
            "--tree",
            "--kf",
        ]
        .iter()
        .copied()
        .filter(|flag| args.iter().any(|a| a == flag))
        .collect();
        if !conflicting.is_empty() {
            eprintln!(
                "error: --cmd is headless and cannot resolve a cell-aware scene. \
                 Conflicting flag(s) passed: {}\n\
                 Use a live engine session (omit --cmd, then attach with byro-dbg) \
                 for cell-aware queries. See #637 / FNV-D5-03.",
                conflicting.join(", "),
            );
            return Err(anyhow::anyhow!(
                "--cmd cannot coexist with cell/asset-loading flags"
            ));
        }
        let input = args.get(cmd_idx + 1).map(|s| s.as_str()).unwrap_or("help");
        let mut world = World::new();
        world.insert_resource(DebugStats::default());
        world.insert_resource(EngineConfig {
            debug_logging: true,
            ..Default::default()
        });
        let registry = build_command_registry();
        world.insert_resource(SystemList(Vec::new()));
        world.insert_resource(registry);
        // CONC-D3-04 / #1786 — `reg` stays held (read) across `execute`;
        // see the lock contract on `ConsoleCommand::execute`.
        let reg = world.resource::<CommandRegistry>();
        let output = reg.execute(&world, input);
        drop(reg);
        for line in &output.lines {
            println!("{}", line);
        }
        return Ok(());
    }

    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = App::new(debug_mode, &args);
    app.bench_frames_target = bench_frames;
    app.bench_hold = bench_hold;
    app.screenshot_path = screenshot_path;
    app.camera_pos_override = camera_pos;
    app.camera_forward_override = camera_forward;
    event_loop.run_app(&mut app)?;

    Ok(())
}

/// Phase 1 of construction (#1670) — boot/config plumbing: build the
/// `World` and install every engine resource + pre-registered component
/// storage. Extracted verbatim from the former 581-LOC `App::new`.
pub(crate) fn build_world(debug_mode: bool, args: &[String]) -> World {
    let mut world = World::new();

    // Register built-in resources.
    world.insert_resource(DeltaTime(0.0));
    world.insert_resource(TotalTime(0.0));
    world.insert_resource(EngineConfig {
        debug_logging: debug_mode || cfg!(debug_assertions),
        ..Default::default()
    });
    world.insert_resource(DebugStats::default());
    world.insert_resource(byroredux_core::ecs::ScratchTelemetry::default());
    world.insert_resource(SkinCoverageStats::default());
    // REND-#1451 — live attenuation tuning, read into the renderer
    // each frame and mutated by the `light.atten` console command.
    // Seed from the config `[defaults]` so a benched knee persists
    // across runs.
    let light_defaults = crate::game_profiles::load_launch_defaults();
    let mut light_tuning = crate::components::LightTuning::default();
    if let Some(knee) = light_defaults.light_atten_knee {
        light_tuning.knee_frac = knee.clamp(0.05, 1.0);
    }
    if let Some(legacy) = light_defaults.light_atten_legacy {
        light_tuning.legacy = legacy;
    }
    world.insert_resource(light_tuning);
    // CPU-side per-frame timings — fence_wait / submit_present /
    // etc. Filled by the binary's RedrawRequested handler after
    // each `draw_frame` from the renderer's `FrameTimings`
    // struct. Surfaces the "GPU stall hidden from per-pass
    // timestamps" diagnostic the debug UI's Metrics panel
    // exposed at Phase 7.
    world.insert_resource(byroredux_core::ecs::CpuFrameTimings::default());
    // Per-system wall-time list (Phase 11). Filled by
    // `Scheduler::run` at the end of each invocation, sorted
    // desc. The egui Metrics panel renders the top entries
    // so the operator can see which ECS system dominates
    // `atw_scheduler_ms`.
    world.insert_resource(byroredux_core::ecs::SchedulerSystemTimings::default());
    // Debug-UI sampler state + the aggregated snapshot. Snapshot is
    // empty until `metrics_sample_system` fires its first tick
    // (~500 ms in), at which point CPU / RAM / VRAM / GPU pass
    // times are filled and refreshed at 2 Hz.
    world.insert_resource(MetricsState::default());
    world.insert_resource(MetricsSnapshot::default());
    // Debug-UI load queue. Always present so the debug-server's
    // `LoadNif` / `LoadInteriorCell` / `LoadExteriorCell`
    // handlers can push into it via `try_resource_mut` without
    // structurally inserting. Drained by `App::step_debug_loads`
    // between frames where `&mut World + &mut VulkanContext` are
    // both held.
    world.insert_resource(byroredux_core::ecs::PendingDebugLoadSlot::default());
    // Phase 5 — game profile registry. Loads
    // `assets/debug_profiles.toml` (engine-shipped defaults)
    // plus `~/.byroredux/profiles.toml` (per-user override).
    // Both files missing = empty registry, never an error.
    world.insert_resource(crate::game_profiles::load_default());
    world.insert_resource(byroredux_core::ecs::SelectedRef::default());
    world.insert_resource(InputState::default());
    world.insert_resource(StringPool::new());
    // #1212 / D1-NEW-01 — FormIdPool is the intern table backing
    // `FormIdComponent` and `World::find_by_form_id`. Every
    // cell-loaded REFR's placement form-id interns through this
    // pool at spawn time so console (`prid <fid>`), debug-server,
    // and future Papyrus `ObjectReference` lookups all resolve.
    world.insert_resource(byroredux_core::form_id::FormIdPool::new());
    world.insert_resource(AnimationClipRegistry::new());
    world.insert_resource(NameIndex::new());
    world.insert_resource(SubtreeCache::new());
    world.insert_resource(CellRootIndex::new());
    world.insert_resource(byroredux_physics::PhysicsWorld::new());
    // M28.5 follow-up — engine-wide contact / KCC tunables. Owned
    // as a resource so a single edit propagates through every
    // collider spawn path (Path A NIF imports, character kinematic
    // capsule) and the KCC offset stays in lockstep. Defaults match
    // the pre-unification inline values.
    world.insert_resource(byroredux_physics::ContactConfig::default());
    // WATAL Phase 2 — engine-canonical water-physics constants
    // (buoyancy density ratio + submerged damping). Game-invariant: no
    // game's WATR authors physics params (docs/engine/watal.md §5.3), so
    // one default resource serves every game. The buoyancy phase of
    // `physics_sync_system` reads it; absent it falls back to the same
    // default, so this insert is the single source of truth.
    world.insert_resource(byroredux_physics::PhysicsWaterConstants::default());
    // M44 Phase 1 — audio world. Init failure (no audio device,
    // CI, headless server) leaves the inner `AudioManager` as
    // `None` and every subsequent audio operation no-ops. Boot
    // never fails on a missing audio device.
    world.insert_resource(byroredux_audio::AudioWorld::new());
    // M44 Phase 3.5 — footstep config. `default_sound` is None
    // until/unless the cell loader (or a future asset-provider
    // hook) decodes a BSA-archived sound and stores it here.
    // Defaults are safe: `None` makes `footstep_system` no-op.
    world.insert_resource(FootstepConfig::default());
    // M44 Phase 3.5 / #932 — `footstep_system` reuses this Vec<Vec3>
    // scratch across frames instead of allocating a fresh one each
    // tick. Preallocated to capacity 32 to cover typical 5-10 NPC
    // walking case without re-growing.
    world.insert_resource(crate::components::FootstepScratch::default());
    // M44 Phase 3.5 — opportunistic footstep BSA load. When the
    // user passed `--sounds-bsa <path>`, decode the canonical
    // dirt-walk WAV and stash the `Arc<StaticSoundData>` in
    // `FootstepConfig.default_sound`. Silently no-op when the
    // flag is absent or the archive isn't openable.
    crate::asset_provider::try_load_default_footstep(&mut world, args);
    // Process-lifetime cache of parsed-and-imported NIF scenes.
    // Persists across cell transitions so repeat visits don't re-
    // parse every clutter mesh. See #381.
    world.insert_resource(crate::cell_loader::NifImportRegistry::new());

    // #880 / CELL-PERF-02 — companion cache for the hierarchical
    // scene-import path used by NPC spawn (`load_nif_bytes_with_
    // skeleton`). Pre-fix every NPC re-parsed the same skeleton +
    // body + hand NIFs from BSA bytes (~280 redundant parses /
    // Megaton load). Different output shape from `CachedNifImport`
    // — that one is the flat-import variant for REFR placements;
    // this one carries the hierarchical `ImportedScene` with its
    // `nodes: Vec<ImportedNode>` so the bone hierarchy spawns
    // correctly.
    world.insert_resource(crate::scene_import_cache::SceneImportCache::new());

    // Pre-register component storages that the physics sync system
    // queries on the first frame (before anything has been inserted).
    world.register::<byroredux_physics::RapierHandles>();
    // WATAL Phase 2 — pre-register `WaterContact` so the buoyancy phase's
    // `query_mut::<WaterContact>().insert(..)` succeeds the first time a
    // body enters water (mirrors the `RapierHandles` pre-register).
    world.register::<byroredux_core::ecs::components::water::WaterContact>();
    // M41.x ragdoll — pre-register so the `ragdoll` command's
    // `query_mut::insert` and the writeback system's queries return
    // `Some` even before any actor has been ragdolled.
    world.register::<byroredux_physics::Ragdoll>();
    world.register::<crate::ragdoll::RagdollTemplate>();
    world.register::<crate::ragdoll::RagdollActive>();
    // M44 Phase 3.5: pre-register footstep emitter storage so
    // `footstep_system`'s `query_mut::<FootstepEmitter>` returns
    // `Some` even before the first emitter is inserted (e.g. on
    // startup with no scene loaded).
    world.register::<crate::components::FootstepEmitter>();

    // M42 — pre-register the Sandbox marker storages so
    // `sandbox_seat_system`'s `query_mut::<Seated>().insert(...)` and the
    // `query::<SandboxBehavior>()` skip-scan resolve even before the first
    // actor spawns (the seat guard depends on `Seated` inserts landing).
    world.register::<byroredux_core::ecs::components::SandboxBehavior>();
    world.register::<byroredux_core::ecs::components::Seated>();
    // Seat reservations (cleared per cell load) + the per-cell sit clip
    // handle (set at cell load where the archive provider lives).
    world.insert_resource(crate::components::SeatReservations::default());
    world.insert_resource(crate::components::SandboxSitClip::default());

    // M42.3 — pre-register the Wander marker + runtime-state storages so
    // `wander_system`'s `query::<WanderBehavior>()` skip-scan and
    // `query_mut::<WanderState>().insert(...)` resolve even before the
    // first wandering actor spawns.
    world.register::<byroredux_core::ecs::components::WanderBehavior>();
    world.register::<byroredux_core::ecs::components::WanderState>();

    // M42.4 — pre-register the Travel marker + runtime-state + terminal
    // storages so `travel_system`'s `query::<TravelBehavior>()` skip-scan,
    // `query_mut::<TravelState>().insert(...)`, and
    // `query_mut::<Traveled>().insert(...)` resolve even before the first
    // traveling actor spawns.
    world.register::<byroredux_core::ecs::components::TravelBehavior>();
    world.register::<byroredux_core::ecs::components::TravelState>();
    world.register::<byroredux_core::ecs::components::Traveled>();

    // M42.5 — pre-register the Follow marker + runtime-state storages so
    // `follow_system`'s `query::<FollowBehavior>()` skip-scan and
    // `query_mut::<FollowState>().insert(...)` resolve even before the
    // first following actor spawns.
    world.register::<byroredux_core::ecs::components::FollowBehavior>();
    world.register::<byroredux_core::ecs::components::FollowState>();

    // M42.6 — pre-register the Escort marker + runtime-state + terminal
    // storages so `escort_system`'s `query::<EscortBehavior>()` skip-scan,
    // `query_mut::<EscortState>().insert(...)`, and
    // `query_mut::<Escorted>().insert(...)` resolve even before the first
    // escorting actor spawns.
    world.register::<byroredux_core::ecs::components::EscortBehavior>();
    world.register::<byroredux_core::ecs::components::EscortState>();
    world.register::<byroredux_core::ecs::components::Escorted>();

    // M42.7 — pre-register the Guard marker + runtime-state storages so
    // `guard_system`'s `query::<GuardBehavior>()` skip-scan and
    // `query_mut::<GuardState>().insert(...)` resolve even before the
    // first guarding actor spawns.
    world.register::<byroredux_core::ecs::components::GuardBehavior>();
    world.register::<byroredux_core::ecs::components::GuardState>();

    // M42.8 — pre-register the Patrol marker + runtime-state storages so
    // `patrol_system`'s `query::<PatrolBehavior>()` skip-scan and
    // `query_mut::<PatrolState>().insert(...)` resolve even before the
    // first patrolling actor spawns.
    world.register::<byroredux_core::ecs::components::PatrolBehavior>();
    world.register::<byroredux_core::ecs::components::PatrolState>();

    // Register scripting component storages.
    byroredux_scripting::register(&mut world);

    // M47.0 Phase 2 — build the script registry mapping SCPT
    // `editor_id` → spawner. Populated here by every demo module
    // exposing a `register_spawners` entry point; the cell
    // loader's per-REFR walk (Phase 3) will look up each REFR's
    // base record's `script_form_id` → SCPT → editor_id against
    // this registry. Inserted as a resource so it survives the
    // whole engine lifetime and stays modifiable by downstream
    // crates that ship additional script translations.
    let mut script_registry = byroredux_scripting::ScriptRegistry::new();
    byroredux_scripting::papyrus_demo::register_spawners(&mut script_registry);
    log::info!(
        "ScriptRegistry initialised with {} editor_id mappings",
        script_registry.len()
    );
    world.insert_resource(script_registry);

    world
}

/// Phase 2 of construction (#1670) — ECS system wiring: build the
/// `Scheduler` and register every stage's systems. Extracted verbatim
/// from the former 581-LOC `App::new` (no outer-`world` dependency — the
/// nested `fn …_dispatch(world: &World, …)` items take their own param).
pub(crate) fn build_scheduler() -> Scheduler {
    // Build the system schedule — stages run sequentially, systems
    // within each stage run in parallel via rayon. All parallel
    // systems declare their access via `add_to_with_access`
    // (M27 migration complete); `undeclared_parallel_count()` is
    // asserted to be 0 after construction below. Exclusive systems
    // (add_exclusive) run serially after each stage's parallel
    // batch and do not participate in the conflict analyzer.
    let mut scheduler = Scheduler::new();
    // M27 Phase 3 — `fly_camera_system` and `character_controller_system`
    // are runtime-mutually-exclusive (each early-returns on the
    // wrong `PlayerMode`), so the scheduler's access analyzer
    // paired them up and surfaced a Transform + PhysicsWorld
    // WriteWrite conflict that's structurally impossible at
    // runtime. `player_controller_system` dispatches to one of
    // the two inner systems per frame; declared accesses are the
    // union of both inner systems' accesses.
    scheduler.add_to_with_access(
        Stage::Early,
        crate::systems::player_controller_system,
        Access::new()
            .reads_resource::<crate::systems::PlayerMode>()
            .reads_resource::<crate::systems::PlayerEntity>()
            .reads_resource::<ActiveCamera>()
            .reads_resource::<InputState>()
            .reads_resource::<byroredux_physics::PhysicsWorld>()
            .writes_resource::<byroredux_physics::PhysicsWorld>()
            // #1787 / CONC-D4-01 — the character controller snapshots
            // `ContactConfig::kcc_offset_bu` once per tick
            // (systems/character.rs); read-only, declared for the same
            // reason as the `physics_sync_system` sibling gap.
            .reads_resource::<byroredux_physics::ContactConfig>()
            .reads::<byroredux_physics::CharacterController>()
            .writes::<byroredux_physics::CharacterController>()
            .reads::<byroredux_physics::RapierHandles>()
            .reads::<Transform>()
            .writes::<Transform>(),
    );
    scheduler.add_to_with_access(
        Stage::Early,
        weather_system,
        Access::new()
            .reads_resource::<crate::components::WeatherDataRes>()
            .writes_resource::<crate::components::WeatherDataRes>()
            .reads_resource::<crate::components::WeatherTransitionRes>()
            .writes_resource::<crate::components::WeatherTransitionRes>()
            .writes_resource::<crate::components::GameTimeRes>()
            .reads_resource::<crate::components::CellLightingRes>()
            .writes_resource::<crate::components::CellLightingRes>()
            .writes_resource::<crate::components::SkyParamsRes>()
            .writes_resource::<crate::components::CloudSimState>(),
    );
    scheduler.add_to_with_access(
        Stage::Early,
        byroredux_scripting::timer_tick_system,
        Access::new()
            .writes::<byroredux_scripting::ScriptTimer>()
            .writes::<byroredux_scripting::TimerExpired>(),
    );
    // M47.0 Phase 1 — R5 papyrus_demo dispatchers. These are
    // event-driven (early-return when no ActivateEvent /
    // OnUpdateEvent / RecurringUpdate is present), so they
    // contribute zero work to frames without scripted activity.
    // Registered as exclusive in Update so they run serially
    // after the parallel batch — keeps M27's zero-conflict
    // report intact without forcing each demo to declare its
    // (component-heavy) access surface. Declared access lands
    // when the demos move to Stage::Script in a follow-up; for
    // now they ride the same "trivial gameplay system =
    // exclusive" lane as spin_system / footstep_system /
    // particle_system. See docs/engine/m47-0-design.md.
    //
    // Signature note: the 6 World-only demos predate engine
    // wiring (R5 was a prototype against unit tests that called
    // them directly). The blanket `impl<F: FnMut(&World, f32)>
    // System for F` requires the `dt` arg, so we adapt via inline
    // closures that drop the unused parameter. Renaming the demo
    // signatures themselves would break ~30 unit-test call sites
    // in papyrus_demo/tests.rs that intentionally call without
    // dt; closures are cheaper.
    // F7 (2026-05-27) — wrap each `&World`-only papyrus-demo
    // system as a local `fn` item so it gets a unique
    // `std::any::type_name`. The previous closures all shared the
    // auto-generated name `byroredux::App::new::{{closure}}`,
    // tripping the scheduler's `has_system(name)` duplicate-name
    // probe and emitting 5+ "duplicate exclusive system" warnings
    // per launch (cosmetic — the systems still all got added).
    // Local fn items each have their own unique type and unique
    // type-name path, so the duplicate check passes naturally.
    fn rumble_on_activate_dispatch(world: &World, _dt: f32) {
        byroredux_scripting::papyrus_demo::rumble_on_activate_system(world)
    }
    fn trigger_detection_dispatch(world: &World, _dt: f32) {
        byroredux_scripting::trigger_detection_system(world)
    }
    fn quest_advance_dispatch(world: &World, _dt: f32) {
        byroredux_scripting::papyrus_demo::quest_advance::quest_advance_system(world)
    }
    // SCR-D6-NEW-02 (#1768) — the runtime scripting systems that were
    // registered (component/resource) but never scheduled. Both ride
    // the same exclusive-in-Update lane as the demo dispatchers above:
    // exclusive registration holds the world serially, so the fragment
    // system's resource-lock sequence never composes with a parallel
    // neighbour. `quest_fragment_dispatch_system` is `&World`-only, so
    // it needs the dt-dropping wrapper; `recurring_update_tick_system`
    // already has the `(&World, f32)` shape and is added directly.
    fn quest_fragment_dispatch(world: &World, _dt: f32) {
        byroredux_scripting::quest_fragment_dispatch_system(world)
    }
    fn dlc2_ttr4a_on_init_dispatch(world: &World, _dt: f32) {
        byroredux_scripting::papyrus_demo::dlc2_ttr4a::dlc2_ttr4a_on_init_system(world)
    }
    fn dlc2_ttr4a_on_update_dispatch(world: &World, _dt: f32) {
        byroredux_scripting::papyrus_demo::dlc2_ttr4a::dlc2_ttr4a_on_update_system(world)
    }
    fn mg07_on_load_dispatch(world: &World, _dt: f32) {
        byroredux_scripting::papyrus_demo::mg07_door::mg07_on_load_system(world)
    }
    fn mg07_on_activate_dispatch(world: &World, _dt: f32) {
        byroredux_scripting::papyrus_demo::mg07_door::mg07_on_activate_system(world)
    }
    scheduler.add_exclusive(Stage::Update, rumble_on_activate_dispatch);
    scheduler.add_exclusive(
        Stage::Update,
        byroredux_scripting::papyrus_demo::rumble_tick_system,
    );
    // M47.2 — trigger detection runs BEFORE quest_advance so an
    // OnTriggerEnterEvent emitted this frame is consumed the same
    // frame (before end-of-frame cleanup drains it).
    scheduler.add_exclusive(Stage::Update, trigger_detection_dispatch);
    scheduler.add_exclusive(Stage::Update, quest_advance_dispatch);
    // Dispatch quest fragments right after the advance that emits the
    // `QuestStageAdvanced` markers, before end-of-frame cleanup drains
    // them (populated live from parsed QUST VMAD fragments, #1739 / `8a70b81a`).
    scheduler.add_exclusive(Stage::Update, quest_fragment_dispatch);
    scheduler.add_exclusive(Stage::Update, dlc2_ttr4a_on_init_dispatch);
    // `recurring_update_tick_system` ticks `RecurringUpdate` and emits
    // `OnUpdateEvent`. It sits between the demo's OnInit (which
    // subscribes via `RegisterForUpdate`) and its OnUpdate consumer so
    // a fired event is handled the same frame, before cleanup drains it.
    scheduler.add_exclusive(Stage::Update, byroredux_scripting::recurring_update_tick_system);
    // CHARAL pool regen (Fatigue/Magicka) — a fixed 60 Hz tick decoupled
    // from the variable frame rate, mirroring `physics_sync_system`'s
    // accumulator (`crates/core/src/character/regen.rs`). No-ops today:
    // `PoolRegenConfig` is only inserted once a game's live `CharacterRuleset`
    // wiring reaches Oblivion (`build_character_ruleset` currently returns
    // `None` for it, per `npc_spawn.rs`) — registered now so the tick is
    // already live the moment that wiring lands.
    scheduler.add_exclusive(Stage::Update, byroredux_core::character::pool_regen_tick_system);
    scheduler.add_exclusive(Stage::Update, dlc2_ttr4a_on_update_dispatch);
    scheduler.add_exclusive(Stage::Update, mg07_on_load_dispatch);
    scheduler.add_exclusive(Stage::Update, mg07_on_activate_dispatch);
    scheduler.add_exclusive(
        Stage::Update,
        byroredux_scripting::papyrus_demo::mg07_door::mg07_tick_system,
    );
    scheduler.add_to_with_access(
        Stage::Update,
        make_animation_system(),
        // M27 — animation_system writes the full set of animated-
        // channel storages (every channel a clip may target). The
        // declaration is the UNION across all paths; individual
        // frames touch a subset depending on which clips are
        // playing. See `byroredux/src/systems/animation.rs:305`.
        Access::new()
            .reads_resource::<byroredux_core::animation::AnimationClipRegistry>()
            .writes_resource::<byroredux_core::animation::AnimationClipRegistry>()
            .reads_resource::<crate::components::SubtreeCache>()
            .writes_resource::<crate::components::SubtreeCache>()
            .reads_resource::<crate::components::NameIndex>()
            .writes_resource::<crate::components::NameIndex>()
            .writes_resource::<byroredux_core::string::StringPool>()
            .reads::<byroredux_core::ecs::Name>()
            .writes::<Transform>()
            .writes::<byroredux_core::animation::RootMotionDelta>()
            .writes::<byroredux_core::ecs::AnimatedVisibility>()
            .writes::<byroredux_core::ecs::AnimatedDiffuseColor>()
            .writes::<byroredux_core::ecs::AnimatedAmbientColor>()
            .writes::<byroredux_core::ecs::AnimatedSpecularColor>()
            .writes::<byroredux_core::ecs::AnimatedEmissiveColor>()
            .writes::<byroredux_core::ecs::AnimatedShaderColor>()
            .writes::<byroredux_core::ecs::AnimatedAlpha>()
            .writes::<byroredux_core::ecs::AnimatedUvTransform>()
            .writes::<byroredux_core::ecs::AnimatedShaderFloat>()
            .writes::<byroredux_core::ecs::AnimatedMorphWeights>()
            .writes::<byroredux_core::ecs::LightSource>()
            .writes::<byroredux_core::animation::AnimationPlayer>()
            .writes::<byroredux_scripting::events::AnimationTextKeyEvents>()
            .writes::<byroredux_core::animation::AnimationStack>(),
    );
    // M27 Phase 3 — `spin_system` writes Transform on entities
    // tagged with `Spinning` (the demo cube). `animation_system`
    // also writes Transform on its own (disjoint) entity set.
    // They never touch the same entity, but the analyzer can't see
    // that — they pair as a WriteWrite Transform conflict. Moving
    // `spin_system` to exclusive sequences it after the Update
    // parallel batch and removes the conflict from the report
    // without changing observable behaviour. Cost: ~µs of lost
    // parallelism on the demo cube; negligible.
    scheduler.add_exclusive(Stage::Update, spin_system);
    // Phase 17 — procedural light flicker. Writes
    // LightSource.intensity + Transform.translation on entities
    // with a LightFlicker companion. Exclusive in Update so it
    // sequences AFTER the parallel batch (no Transform conflict
    // with animation_system / spin_system) but BEFORE
    // PostUpdate's transform propagation reads the result.
    scheduler.add_exclusive(Stage::Update, animate_lights_system);
    scheduler.add_to_with_access(
        Stage::PostUpdate,
        make_transform_propagation_system(),
        Access::new()
            .reads::<byroredux_core::ecs::Parent>()
            .reads::<byroredux_core::ecs::Children>()
            // WRITE (was read): the system drains the per-entity
            // Transform change-tracking dirty set, which needs &mut on
            // the storage. Local transforms are still only read.
            .writes::<Transform>()
            .writes::<byroredux_core::ecs::GlobalTransform>(),
    );
    // M44 Phase 3.5: footstep dispatch. Reads `GlobalTransform`
    // for the world-space spawn position, so it MUST run after
    // `make_transform_propagation_system()` — otherwise the
    // footstep lands at last-frame's pose. Pre-#848 this was
    // registered in `Stage::Update`, ahead of propagation; the
    // commit comment claimed "~3 cm of motion" stale but that
    // underestimated by ~100× for fly-cam boost (~3 game units
    // / frame at 60 FPS, audible spatial-pan offset on a
    // ~50-200-unit interior cell). Registered as exclusive so
    // it sequences AFTER the PostUpdate parallel batch — same
    // pattern as `particle_system` / `billboard_system` /
    // `world_bound_propagation_system` below. See #848.
    scheduler.add_exclusive(Stage::PostUpdate, footstep_system);
    // Particle simulation runs after transform propagation so emitter
    // entities have their final world-space spawn origin (#401).
    scheduler.add_exclusive(Stage::PostUpdate, particle_system);
    // M42 — sandbox seat procedure. GATED OFF by default (opt in with
    // `BYRO_SANDBOX_SIT=1`). The seat placement + clip-swap pipeline is fully
    // verified (live bone inspection: actors land on the correct furniture
    // marker and the sit clip *is* applied — L-thigh matches the authored
    // folded pose). M42.1 fixed the earlier float bug (the generic
    // `dynamicidle_*` sit loops carry no pelvis/root channel) by holding the
    // FNV/FO3 sit-**enter** transition clip's final frame instead, which does
    // lower `Bip01`/`NonAccum` onto the seat; see `systems::sandbox` module
    // docs for the full mechanism. The rest of the M42 foundation (Sandbox
    // package tagging, `Furniture` markers, `Seated`, resources) stays live
    // regardless. Runs after transform propagation, same exclusive lane as
    // the systems above.
    if std::env::var_os("BYRO_SANDBOX_SIT").is_some() {
        log::info!(
            "BYRO_SANDBOX_SIT set — enabling sandbox seat-snap \
             (grounded sit-enter pose on FNV/FO3; see systems::sandbox docs for other games)"
        );
        scheduler.add_exclusive(Stage::PostUpdate, crate::systems::make_sandbox_seat_system());
    }
    // M42.3 — Wander locomotion. GATED OFF by default (opt in with
    // `BYRO_WANDER=1`), mirroring `BYRO_SANDBOX_SIT` above. Straight-line
    // walk-to-point, no pathing/NAVM, no animation-clip swap — see
    // `systems::wander` module docs for the full v0-scope list. Same
    // exclusive PostUpdate lane, after transform propagation, as
    // `sandbox_seat_system`; the two never touch the same actor (an NPC's
    // active package is a single winning `PackRecord`, so `SandboxBehavior`
    // and `WanderBehavior` are mutually exclusive), so their relative
    // order doesn't matter.
    if std::env::var_os("BYRO_WANDER").is_some() {
        log::info!("BYRO_WANDER set — enabling NPC wander locomotion (M42.3 v0)");
        scheduler.add_exclusive(Stage::PostUpdate, crate::systems::make_wander_system());
    }
    // M42.4 — Travel locomotion. GATED OFF by default (opt in with
    // `BYRO_TRAVEL=1`), mirroring `BYRO_WANDER`/`BYRO_SANDBOX_SIT` above.
    // Shares `wander_system`'s straight-line walk primitive via
    // `systems::locomotion::step_toward`, but walks once to a destination
    // and stops (terminal `Traveled` marker) instead of repeating — see
    // `systems::travel` module docs for the resolution/fallback mechanism
    // and the full v0-scope list. Same exclusive PostUpdate lane, after
    // transform propagation; Sandbox/Wander/Travel never touch the same
    // actor (a single winning `PackRecord` per NPC), so relative order
    // among the three doesn't matter.
    if std::env::var_os("BYRO_TRAVEL").is_some() {
        log::info!("BYRO_TRAVEL set — enabling NPC travel locomotion (M42.4 v0)");
        scheduler.add_exclusive(Stage::PostUpdate, crate::systems::make_travel_system());
    }
    // M42.5 — Follow locomotion. GATED OFF by default (opt in with
    // `BYRO_FOLLOW=1`), mirroring `BYRO_TRAVEL`/`BYRO_WANDER` above.
    // Shares the same `step_toward` locomotion primitive, but tracks a
    // *live* target's position every tick instead of a frozen destination
    // (Travel) or a hash-picked point (Wander) — see `systems::follow`
    // module docs for the PTDT target-resolution mechanism and the full
    // v0-scope list. Same exclusive PostUpdate lane, after transform
    // propagation; Sandbox/Wander/Travel/Follow never touch the same
    // actor (a single winning `PackRecord` per NPC), so relative order
    // among the four doesn't matter.
    if std::env::var_os("BYRO_FOLLOW").is_some() {
        log::info!("BYRO_FOLLOW set — enabling NPC follow locomotion (M42.5 v0)");
        scheduler.add_exclusive(Stage::PostUpdate, crate::systems::make_follow_system());
    }
    // M42.6 — Escort locomotion. GATED OFF by default (opt in with
    // `BYRO_ESCORT=1`), mirroring `BYRO_FOLLOW`/`BYRO_TRAVEL`/`BYRO_WANDER`
    // above. Shares the same `step_toward` locomotion primitive across two
    // phases — collect a live PTDT target (like Follow), then lead it to a
    // frozen PLDT destination and stop (like Travel, terminal `Escorted`
    // marker) — see `systems::escort` module docs for the full mechanism
    // and v0-scope list. Same exclusive PostUpdate lane, after transform
    // propagation; Sandbox/Wander/Travel/Follow/Escort never touch the
    // same actor (a single winning `PackRecord` per NPC), so relative
    // order among the five doesn't matter.
    if std::env::var_os("BYRO_ESCORT").is_some() {
        log::info!("BYRO_ESCORT set — enabling NPC escort locomotion (M42.6 v0)");
        scheduler.add_exclusive(Stage::PostUpdate, crate::systems::make_escort_system());
    }
    // M42.7 — Guard locomotion. GATED OFF by default (opt in with
    // `BYRO_GUARD=1`), mirroring `BYRO_ESCORT`/`BYRO_FOLLOW`/`BYRO_TRAVEL`/
    // `BYRO_WANDER` above. Reuses `travel_system`'s anchor-resolution logic
    // but never reaches a terminal state — holds the anchor indefinitely,
    // returning if displaced beyond its radius — see `systems::guard`
    // module docs. Same exclusive PostUpdate lane, after transform
    // propagation; Sandbox/Wander/Travel/Follow/Escort/Guard never touch
    // the same actor (a single winning `PackRecord` per NPC), so relative
    // order among the six doesn't matter.
    if std::env::var_os("BYRO_GUARD").is_some() {
        log::info!("BYRO_GUARD set — enabling NPC guard locomotion (M42.7 v0)");
        scheduler.add_exclusive(Stage::PostUpdate, crate::systems::make_guard_system());
    }
    // M42.8 — Patrol locomotion. GATED OFF by default (opt in with
    // `BYRO_PATROL=1`), mirroring the gates above. v0 Patrol is Wander's
    // exact random-point-in-radius algorithm under a different procedure
    // tag — no patrol-route data is decoded anywhere in this codebase, so
    // there is nothing to differentiate it on yet; see `systems::patrol`
    // module docs. Same exclusive PostUpdate lane.
    if std::env::var_os("BYRO_PATROL").is_some() {
        log::info!("BYRO_PATROL set — enabling NPC patrol locomotion (M42.8 v0, aliases Wander's algorithm)");
        scheduler.add_exclusive(Stage::PostUpdate, crate::systems::make_patrol_system());
    }
    // PostUpdate ordering contract (#1375 invariant pin):
    //   1. transform_propagation — BFS GlobalTransform composition
    //   2. make_billboard_system  — overwrites billboard GT rotations
    //                              (camera-motion gated; see #1374)
    //   3. make_world_bound_propagation_system — drains GT dirty set,
    //      folds WorldBounds from the final per-frame transforms
    //
    // INVARIANT: no Stage::Late system may write GlobalTransform on
    // a LocalBound-bearing entity. If it does, that entity's
    // WorldBound will silently lag one frame because bound
    // propagation's GT drain fires before the Late write. Today
    // camera_follow_system + audio emitters write GT in Late but
    // carry no LocalBound — the lag is benign. Any future Late
    // system that writes GT on a bounded entity must either be
    // promoted to PostUpdate (before bounds) or accept one-frame
    // stale WorldBounds for that entity.
    scheduler.add_exclusive(Stage::PostUpdate, make_billboard_system());
    // Bound propagation runs last in PostUpdate so it sees final
    // world transforms (including billboard rotations). See #217.
    scheduler.add_exclusive(Stage::PostUpdate, make_world_bound_propagation_system());
    // Submersion detection runs in PostUpdate after bound
    // propagation so the camera's GlobalTransform is already
    // current for the frame. Reads `WaterPlane`/`WaterVolume` +
    // `GlobalTransform`, writes `SubmersionState` on the active
    // camera entity. Downstream consumers (audio low-pass send,
    // underwater composite tint) read the result later in the
    // frame.
    scheduler.add_exclusive(Stage::PostUpdate, crate::systems::submersion_system);
    scheduler.add_to_with_access(
        Stage::Physics,
        byroredux_physics::physics_sync_system,
        Access::new()
            .reads_resource::<byroredux_physics::PhysicsWorld>()
            .writes_resource::<byroredux_physics::PhysicsWorld>()
            // WATAL Phase 2 — the buoyancy phase reads the engine water
            // constants resource (declaration completeness; read-only).
            .reads_resource::<byroredux_physics::PhysicsWaterConstants>()
            // #1787 / CONC-D4-01 — `register_newcomers` snapshots
            // `ContactConfig` once per batch (kcc_offset_bu / trimesh
            // flags); read-only, but must be declared so a future
            // parallel system that writes it is caught by the
            // conflict analyzer instead of silently racing.
            .reads_resource::<byroredux_physics::ContactConfig>()
            .reads::<byroredux_core::ecs::components::CollisionShape>()
            .reads::<byroredux_core::ecs::components::RigidBodyData>()
            .reads::<byroredux_core::ecs::GlobalTransform>()
            .reads::<byroredux_physics::RapierHandles>()
            .writes::<byroredux_physics::RapierHandles>()
            .writes::<Transform>()
            // WATAL Phase 2 — the buoyancy phase reads the water plane
            // components and writes per-body `WaterContact`.
            .reads::<byroredux_core::ecs::components::water::WaterPlane>()
            .reads::<byroredux_core::ecs::components::water::WaterVolume>()
            .reads::<byroredux_core::ecs::components::water::WaterFlow>()
            .writes::<byroredux_core::ecs::components::water::WaterContact>()
            // #1787 / CONC-D4-01 — the #1698 `BYRO_PROFILE_FALLERS`
            // opt-in diagnostic (`dump_awake_fallers`) reads these
            // three, gated behind an env var + one-shot AtomicBool but
            // still part of the system's true read surface — the
            // analyzer can't see the runtime gate.
            .reads::<byroredux_core::ecs::components::RenderLayer>()
            .reads::<byroredux_core::ecs::components::FormIdComponent>()
            .reads::<byroredux_core::ecs::components::PhysicsSourceForm>()
            .reads_resource::<byroredux_core::form_id::FormIdPool>(),
    );
    // M28.5 — camera follow runs in Stage::Late, AFTER
    // `physics_sync_system` has settled the kinematic body's
    // post-step pose. Must run BEFORE `audio_system` /
    // `submersion_system` (both read camera GlobalTransform).
    // The character system writes both Transform and
    // GlobalTransform on the camera to bypass the missing
    // late-stage propagation pass.
    scheduler.add_to_with_access(
        Stage::Late,
        crate::systems::camera_follow_system,
        Access::new()
            .reads_resource::<crate::systems::PlayerEntity>()
            .reads_resource::<ActiveCamera>()
            .reads_resource::<InputState>()
            .reads::<byroredux_physics::CharacterController>()
            .reads::<byroredux_core::ecs::GlobalTransform>()
            .writes::<byroredux_core::ecs::GlobalTransform>()
            .reads::<Transform>()
            .writes::<Transform>(),
    );
    // M41.x — ragdoll writeback. Stage::Late guarantees it runs after
    // `physics_sync_system` (Stage::Physics) has stepped the multibody
    // *and* after PostUpdate transform propagation, so overwriting each
    // ragdoll bone's GlobalTransform with the simulated pose is the last
    // word before render (no propagation/animation skip needed).
    //
    // Registered **exclusive** (not in the Late parallel batch): it
    // writes GlobalTransform, and so does `camera_follow_system` in the
    // same stage — declaring both in the parallel batch is a WriteWrite
    // conflict (#1601). They write entity-disjoint sets (camera entity
    // vs. ragdoll bones), so ordering is irrelevant; exclusive sequencing
    // keeps the scheduler's known_conflict_count() at 0. Matches the
    // existing add_exclusive treatment of audio_system / event_cleanup.
    scheduler.add_exclusive(Stage::Late, crate::ragdoll::ragdoll_writeback_system);
    // M44 Phase 6 — cell-acoustics → reverb send (#846). Runs
    // before `audio_system` so any new spatial track constructed
    // this frame picks up the right send level. Already-playing
    // sounds keep their construction-time send (kira contract);
    // long-running ambients across interior/exterior transitions
    // are tracked separately in AUD-D5-NEW-06.
    scheduler.add_to_with_access(
        Stage::Late,
        crate::systems::reverb_zone_system,
        Access::new()
            .reads_resource::<crate::components::CellLightingRes>()
            .writes_resource::<byroredux_audio::AudioWorld>(),
    );
    // M44 Phase 1 — audio update runs in Stage::Late so it sees
    // final world transforms after propagation. The Phase 1 body
    // is a stub (see byroredux_audio::audio_system); future
    // phases (one-shot dispatch, listener pose sync, looping
    // emitter lifecycle) flesh it out without touching the
    // schedule wiring.
    //
    // M27 Phase 3 — registered as **exclusive** so it sequences
    // after the Late parallel batch. The ordering comment at
    // line 650-656 above ("MUST run BEFORE audio_system" /
    // "Must run BEFORE audio_system") encodes a real
    // dependency that the parallel batch can't guarantee on its
    // own; exclusive sequencing makes the dependency structural.
    // Side effect: removes two analyzer-visible conflicts
    // (camera_follow ↔ audio on GlobalTransform; reverb_zone ↔
    // audio on AudioWorld) — exclusive systems aren't paired
    // against anything in the access report.
    scheduler.add_exclusive(Stage::Late, byroredux_audio::audio_system);
    scheduler.add_to_with_access(
        Stage::Late,
        log_stats_system,
        Access::new()
            .reads_resource::<TotalTime>()
            .reads_resource::<DeltaTime>()
            .reads_resource::<DebugStats>(),
    );
    // Debug-UI metrics sampler — throttles itself to ~2 Hz, so the
    // per-frame cost is a single resource read + compare. On a
    // sample tick it walks sysinfo + the gpu-allocator block list
    // and writes the snapshot read by the protocol / TUI / egui
    // overlay.
    scheduler.add_to_with_access(
        Stage::Late,
        metrics_sample_system,
        Access::new()
            .reads_resource::<TotalTime>()
            .reads_resource::<SkinCoverageStats>()
            .reads_resource::<byroredux_renderer::vulkan::allocator::AllocatorResource>()
            .reads_resource::<byroredux_renderer::vulkan::allocator::GpuMemoryBudget>()
            .writes_resource::<MetricsState>()
            .writes_resource::<MetricsSnapshot>(),
    );
    scheduler.add_exclusive(Stage::Late, byroredux_scripting::event_cleanup_system);

    scheduler
}

/// Phase 3 of construction (#1670) — post-build runtime registries: the
/// scheduler-derived resources (`SystemList`, access report) plus the
/// console-command and save registries. Reads the built `scheduler`
/// immutably; mutates `world`. Extracted verbatim from `App::new`.
pub(crate) fn install_runtime_registries(world: &mut World, scheduler: &Scheduler) {
    // #1394 — guard against silently re-introducing undeclared parallel
    // systems.  All parallel-batch entries must use add_to_with_access;
    // any future add_to() call will trip this in debug builds before
    // the Unknown row appears in the sys.accesses conflict report.
    let report_snapshot = scheduler.access_report();
    debug_assert_eq!(
        report_snapshot.undeclared_parallel_count(),
        0,
        "undeclared parallel system detected — use add_to_with_access instead of add_to"
    );
    // #1602 — also gate the *declared*-conflict and unknown-pair
    // invariants, not just undeclared systems. The old guard checked
    // only undeclared_parallel_count(), which is why a declared
    // WriteWrite conflict (#1601: ragdoll/camera both writing
    // GlobalTransform in the Late parallel batch) slipped through.
    // Either run `sys.accesses` to see the offending pair, or make one
    // side exclusive.
    debug_assert_eq!(
        report_snapshot.known_conflict_count(),
        0,
        "declared access conflict between two parallel same-stage systems \
         — make one side exclusive or split the access (see sys.accesses)"
    );
    debug_assert_eq!(
        report_snapshot.unknown_pair_count(),
        0,
        "unknown (undeclared) parallel pairing detected — declare both sides' access"
    );

    // Store system names + console commands as resources.
    let system_names: Vec<String> = scheduler
        .system_names()
        .iter()
        .map(|s| s.to_string())
        .collect();
    world.insert_resource(SystemList(system_names));
    // R7: snapshot the per-stage access report once after the
    // schedule is built. Read by `sys.accesses` to surface
    // declared-access conflicts to the operator.
    world.insert_resource(byroredux_core::ecs::SchedulerAccessReport(report_snapshot));
    world.insert_resource(build_command_registry());
    // M45 — install the save registry + slot directory so the
    // `save` / `save.info` console commands can operate. Saves live
    // under `<cwd>/saves`; the ring keeps the last 10 quicksaves so
    // a fresh save never immediately clobbers the previous good one.
    world.insert_resource(crate::save_io::build_save_registry());
    world.insert_resource(crate::save_io::SaveState::new(
        std::path::PathBuf::from("saves"),
        10,
    ));
    // M45.1 — deferred live-load slot, drained by `step_save_loads`
    // between frames (the `load` command has only `&World`).
    world.insert_resource(crate::save_io::PendingSaveLoadSlot::default());
    // M45.1 refinement — player/camera pose, refreshed each frame by
    // `capture_player_pose` and rode along in the snapshot so `load`
    // restores the saved spot instead of the cell's default door.
    world.insert_resource(crate::save_io::PlayerPose::default());
}

/// Phase 20 — `--game <key>` CLI expansion. Looks up the named
/// profile in `assets/debug_profiles.toml` (+ per-user override at
/// `~/.byroredux/profiles.toml`), resolves `<games-root>/<subdir>`,
/// and appends synthetic `--esm` / `--bsa` / `--textures-bsa` /
/// `--materials-ba2` args to the input list. The user's own flags
/// stay at the FRONT — first-occurrence-wins for unique args means
/// a user `--esm Custom.esm` beats the profile default; additive
/// args (`--bsa`) take both. No-op when `--game` is absent.
///
/// `--games-root <path>` and `BYROREDUX_GAMES_ROOT` env var
/// override the default `/mnt/data/SteamLibrary/steamapps/common`.
/// Both stripped from the returned arg list — the rest of the
/// engine doesn't read them.
///
/// Missing profile / non-existent paths → log a warning and
/// return the input unchanged so the engine still boots (the
/// user can correct + retry without a re-build cycle).
fn expand_game_profile_args(mut args: Vec<String>) -> Vec<String> {
    // Launch defaults from the `[defaults]` table (profiles.toml,
    // shipped + per-user override). Let an explicit `--game` win; fall
    // back to `[defaults].game` ONLY when no other content-loading flag
    // is present, so `--mesh foo.nif`, `--cmd`, an explicit `--esm`, or
    // a master-only run aren't hijacked into loading the default cell.
    let defaults = crate::game_profiles::load_launch_defaults();
    let has_other_load_flags = ["--esm", "--mesh", "--tree", "--kf", "--cmd", "--master"]
        .iter()
        .any(|f| args.iter().any(|a| a == f));
    let game_key = match parse_string_arg(&args, "--game") {
        Some(k) => k,
        None => match defaults.game.clone() {
            Some(k) if !has_other_load_flags => {
                eprintln!("[defaults] game = {k:?} (no --game / load flag given)");
                k
            }
            _ => return args,
        },
    };
    // `--games-root` CLI wins; else the config `[defaults].games_root`.
    let games_root_cli =
        parse_string_arg(&args, "--games-root").or_else(|| defaults.games_root.clone());

    // Strip the two new flags + their values from the returned
    // args; downstream code doesn't recognise them.
    args = strip_flag_and_value(args, "--game");
    args = strip_flag_and_value(args, "--games-root");

    // Load profile registry from disk. Same loader the App uses
    // at world init; safe to call before logging is initialised
    // (uses eprintln internally on failure).
    let registry = crate::game_profiles::load_default();
    let entry = match registry.get(&game_key) {
        Some(e) => e.clone(),
        None => {
            eprintln!(
                "--game {}: profile not found in `assets/debug_profiles.toml` \
                 or `~/.byroredux/profiles.toml`. Known keys: {:?}",
                game_key,
                registry.iter().map(|(k, _)| k).collect::<Vec<_>>()
            );
            return args;
        }
    };

    let games_root = crate::game_profiles::resolve_games_root(games_root_cli.as_deref());
    let data_dir = crate::game_profiles::resolve_profile_root(&entry, &games_root);

    if data_dir.as_os_str().is_empty() {
        eprintln!(
            "--game {}: profile carries neither `root` nor `subdir`; cannot resolve data dir",
            game_key,
        );
        return args;
    }
    if !data_dir.exists() {
        eprintln!(
            "--game {}: resolved data dir does not exist: {} \
             (set --games-root, BYROREDUX_GAMES_ROOT env var, or override \
              the profile's `root` in ~/.byroredux/profiles.toml)",
            game_key,
            data_dir.display(),
        );
        // Fall through anyway — let downstream loaders report
        // specific missing files for clearer diagnostics.
    }

    // Append profile-derived args. User's earlier --esm wins on
    // first-occurrence-wins; additive --bsa flags compose.
    eprintln!(
        "--game {} expanding from {} (data dir: {})",
        game_key,
        entry.name,
        data_dir.display(),
    );
    let join_arg =
        |archive: &str| -> String { data_dir.join(archive).to_string_lossy().into_owned() };

    args.push("--esm".to_string());
    args.push(join_arg(&entry.esm));
    for bsa in &entry.default_bsas {
        args.push("--bsa".to_string());
        args.push(join_arg(bsa));
    }
    for bsa in &entry.default_textures_bsas {
        args.push("--textures-bsa".to_string());
        args.push(join_arg(bsa));
    }
    for bsa in &entry.default_materials_bsas {
        args.push("--materials-ba2".to_string());
        args.push(join_arg(bsa));
    }

    // Default cell: inject `[defaults].cell` only when the profile was
    // resolved and no explicit location flag is present. A bare
    // `--game fnv` (or a no-arg default-game launch) then boots
    // straight into the configured cell.
    if let Some(cell) = &defaults.cell {
        let has_location = ["--cell", "--grid", "--wrld"]
            .iter()
            .any(|f| args.iter().any(|a| a == f));
        if !has_location {
            eprintln!("[defaults] cell = {cell:?} (no --cell / --grid / --wrld given)");
            args.push("--cell".to_string());
            args.push(cell.clone());
        }
    }
    args
}

/// Remove every `<flag> <value>` pair from the args list. Used
/// by `--game` expansion to strip the new flags before downstream
/// parsers see them.
fn strip_flag_and_value(args: Vec<String>, flag: &str) -> Vec<String> {
    let mut out = Vec::with_capacity(args.len());
    let mut iter = args.into_iter();
    while let Some(a) = iter.next() {
        if a == flag {
            // Also discard the next arg (the value).
            let _ = iter.next();
            continue;
        }
        out.push(a);
    }
    out
}
