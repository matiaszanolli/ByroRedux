//! Boot/config plumbing split out of `main.rs` (#1858 / TD1-003):
//! CLI arg parsing + expansion, `World`/`Scheduler` construction, and
//! the winit event-loop kickoff. `App`'s `ApplicationHandler` impl and
//! per-frame stepping stay in `main.rs` / `app_step.rs` respectively.
//!
//! #3855 — split from a single 3229-line `boot.rs` into one file per
//! concern. The five `register_*_systems` bodies #3739 had already carved
//! out of `build_scheduler` became `schedule/{early,update,post_update,
//! physics,late}.rs` verbatim; `build_world`, `install_runtime_registries`
//! and the CLI expanders each got their own file. Every `pub(crate)` entry
//! point is re-exported below, so no call site outside this directory
//! changed.

mod cli;
mod registries;
mod schedule;
mod world;

use cli::{expand_boot_request, expand_game_profile_args};
pub(crate) use registries::install_runtime_registries;
pub(crate) use schedule::build_scheduler;
pub(crate) use world::build_world;

use anyhow::Result;
use byroredux_core::console::CommandRegistry;
use byroredux_core::ecs::{DebugStats, EngineConfig, SystemList, World};
use winit::event_loop::{ControlFlow, EventLoop};

use crate::cli_args::{parse_renderer_config, parse_string_arg, parse_vec3_arg};
use crate::commands::build_command_registry;
use crate::App;

/// Every source file this module was split across, concatenated (#3855).
///
/// The scheduler invariants in this crate are pinned by *source-shape*
/// tests — the declarations they check are only visible as text, since the
/// live boot path wants a Vulkan device and on-disk game data. Before the
/// split those tests read `include_str!("boot.rs")`; a per-file
/// `include_str!` would now silently narrow each one to whichever fragment
/// it happened to land in, and a registration moving between stage files
/// would make the assertion vacuous rather than red. One concatenation keeps
/// every existing assertion looking at the same text it always did.
///
/// **The order below is load-bearing.** Each source-shape module truncates
/// this string at its *own* `mod` declaration, so that its own mentions of a
/// system name cannot be what a scan finds. That convention only holds while
/// every production registration appears *before the earliest such
/// declaration* (the pin's own precise phrasing — **not** "before the first
/// test module's text": `world.rs` and `cli.rs` each carry their own
/// `#[cfg(test)]` blocks well before the tail, and those don't need to be
/// truncation sentinels at all, only the ones a `SOURCES` text-scan actually
/// relies on truncating before — #4088) — so the scheduler test modules
/// that DO need to be sentinels, all living in `schedule/mod.rs`, must come
/// last, after the five stage files whose registrations they assert on.
/// Ordering `schedule/mod.rs` ahead of them (the natural `mod`-declaration
/// order) truncates the whole schedule away and every scan finds nothing:
/// six tests failed exactly that way while this split was being made.
#[cfg(test)]
pub(crate) const SOURCES: &str = concat!(
    // Process entry, then the world, then each stage in registration order —
    // mirroring the layout of the single `boot.rs` these came from.
    include_str!("mod.rs"),
    include_str!("world.rs"),
    include_str!("schedule/early.rs"),
    include_str!("schedule/update.rs"),
    include_str!("schedule/post_update.rs"),
    include_str!("schedule/physics.rs"),
    include_str!("schedule/late.rs"),
    include_str!("registries.rs"),
    include_str!("cli.rs"),
    // Last: `build_scheduler` is an 18-line orchestrator that registers
    // nothing itself, and the four test modules sharing its file must trail
    // every registration they read.
    include_str!("schedule/mod.rs"),
);

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

    // `--boot <path>` — the launcher handoff — expands first, so a request
    // may emit `--game <key>` and reuse the profile expander below rather
    // than duplicating archive fan-out. Generated flags append after the
    // user's own argv, keeping explicit command-line flags in control. See
    // `docs/engine/launcher.md` §2.
    //
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
    let args: Vec<String> =
        expand_game_profile_args(expand_boot_request(std::env::args().collect())?);
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
    if bench_frames == Some(0) {
        anyhow::bail!("--bench-frames must be greater than zero");
    }

    // --screenshot PATH: when set, request a screenshot on the bench
    // exit frame (requires --bench-frames) and write to PATH before
    // quitting. No-op without --bench-frames. Used for offline
    // rendering diagnostics / visual regression baselines.
    let screenshot_path = parse_string_arg(&args, "--screenshot");

    // Seed a named correctness view before the first rendered frame. This is
    // primarily useful for deterministic captures of very large scenes: a
    // categorical material view avoids entering the full RT path merely to
    // reach a later `--bench-hold` debug session.
    let initial_render_debug_mode = parse_string_arg(&args, "--render-debug-mode")
        .map(|mode| {
            mode.parse::<byroredux_renderer::RenderDebugMode>()
                .map_err(|error| anyhow::anyhow!(error))
        })
        .transpose()?;

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

    // --groundcover-debug-points: render the scatter's accepted candidate
    // points instead of blades. §9's Phase 1 view — "this is where the
    // distribution is judged, before any blade exists" — and still the right
    // tool afterwards, because a blade's silhouette hides the distribution it
    // was placed by. Points are coloured by `d_ground`, so the view answers
    // "what did the field evaluate to here" and not only "is there a blade".
    let groundcover_debug_points = args.iter().any(|a| a == "--groundcover-debug-points");

    // --groundcover-off: skip the scatter and the draw entirely. The A/B
    // switch — ground cover is a whole-screen change to every exterior, so
    // "is this the grass or was it already like that" has to be answerable
    // without rebuilding.
    let groundcover_off = args.iter().any(|a| a == "--groundcover-off");

    // --bench-groundcover-sampling [samples_per_thread,blades_per_chunk]:
    // run the EXAL ground-cover §11.1 terrain-attribute sampling bench
    // (#4052) alongside the normal frame, and print a `groundcover-bench:`
    // row per measured variant with the `bench:` summary.
    //
    // Needs `--bench-frames N` for the same reason `--bench-camera` does —
    // the summary is what emits the rows — and needs a loaded **exterior**
    // worldspace, since the thing being sampled is LAND terrain. Pair it with
    // `--bench-hold` to keep the process alive for `byro-dbg`.
    let groundcover_bench = if args.iter().any(|a| a == "--bench-groundcover-sampling") {
        let mut config = crate::bench::GroundcoverBenchConfig::default();
        if let Some(spec) = parse_string_arg(&args, "--bench-groundcover-sampling") {
            // The flag takes an OPTIONAL `s,b` argument, so a bare flag is
            // followed by whatever came next on the command line (or by
            // nothing). Only a well-formed pair overrides the defaults;
            // anything else is the next flag, not a malformed argument.
            if let Some((s, b)) = spec.split_once(',') {
                match (s.trim().parse::<u32>(), b.trim().parse::<u32>()) {
                    (Ok(s), Ok(b)) if s > 0 && b > 0 => {
                        config.samples_per_thread = s;
                        config.blades_per_chunk = b;
                    }
                    _ => anyhow::bail!(
                        "--bench-groundcover-sampling takes an optional \
                         `<samples_per_thread>,<blades_per_chunk>` pair of positive \
                         integers; got '{spec}'"
                    ),
                }
            }
        }
        if bench_frames.is_none() {
            log::warn!(
                "--bench-groundcover-sampling is inactive without --bench-frames \
                 (the `groundcover-bench:` rows are emitted with the bench summary)"
            );
        }
        Some(config)
    } else {
        None
    };

    // --camera-pos x,y,z + --camera-forward x,y,z — override the
    // auto-computed initial camera pose. Useful for capturing specific
    // framing in bench mode without needing interactive WASD input.
    // Both args are `,`-separated floats. Pass one or both; missing
    // forward defaults to `-Z` toward the origin.
    let camera_pos = parse_vec3_arg(&args, "--camera-pos");
    let camera_forward = parse_vec3_arg(&args, "--camera-forward");

    // --bench-camera <static|pan|orbit|dolly|grid-cross|cut> — drive the camera along a
    // deterministic, frame-indexed path for the length of a `--bench-frames`
    // run. Temporal reconstruction only misbehaves when the camera moves, and
    // neither the fly camera (needs mouse capture) nor the character rig
    // (needs a player) runs in a headless bench, so without this the image-
    // quality harness can only ever measure a parked camera. No-op without
    // `--bench-frames`, since the path is normalized on the run length.
    let requested_bench_camera = match parse_string_arg(&args, "--bench-camera") {
        Some(spec) => match spec.parse::<crate::bench_camera::BenchCameraPath>() {
            Ok(path) => Some(path),
            Err(error) => anyhow::bail!("{error}"),
        },
        None => None,
    };
    if requested_bench_camera.is_some() && bench_frames.is_none() {
        log::warn!("--bench-camera is inactive without --bench-frames (the path is normalized over the run length)");
    }

    // --bench-mode owns both clocks that can change scene state. Explicit
    // modes are the benchmark-of-record path; canonical legacy combinations
    // are inferred during migration so old screenshot/smoke tooling remains
    // usable while every emitted row still carries an honest mode name.
    let requested_bench_mode = match parse_string_arg(&args, "--bench-mode") {
        Some(spec) => match spec.parse::<crate::bench::BenchMode>() {
            Ok(mode) => Some(mode),
            Err(error) => anyhow::bail!("{error}"),
        },
        None => None,
    };
    let legacy_fixed_dt = std::env::var("BYROREDUX_FIXED_DT").ok();
    let bench_selection = crate::bench::resolve_bench_selection(
        bench_frames.is_some(),
        requested_bench_mode,
        requested_bench_camera,
        legacy_fixed_dt.as_deref(),
    )
    .map_err(anyhow::Error::msg)?;
    let bench_mode = bench_selection.map(|selection| selection.mode);
    let bench_camera = bench_selection
        .and_then(|selection| selection.camera)
        .or(requested_bench_camera);

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

    if let Some(selection) = bench_selection {
        let camera = selection
            .camera
            .map_or_else(|| "free".to_owned(), |path| path.to_string());
        log::info!(
            "benchmark mode: {} (dt={}, camera={}, gate={})",
            selection.mode,
            selection.mode.dt_label(),
            camera,
            selection.mode.gate_label(),
        );
        if selection.inferred && (legacy_fixed_dt.is_some() || requested_bench_camera.is_some()) {
            log::warn!(
                "legacy benchmark controls inferred as '{}'; pass --bench-mode {} explicitly",
                selection.mode,
                selection.mode,
            );
        }
    }

    let renderer_config = parse_renderer_config(&args)?;
    log::info!("Renderer upscaler selection: {}", renderer_config.upscaler);
    if renderer_config.rt_test_lod_scale_bits.is_some()
        || renderer_config.rt_test_lod_telemetry
        || renderer_config.rt_test_ray_quality_tier.is_some()
    {
        log::warn!(
            "RT TEST LOD diagnostics active: scale={:?} telemetry={} quality_tier={:?}",
            renderer_config.rt_test_lod_scale_bits.map(f32::from_bits),
            renderer_config.rt_test_lod_telemetry,
            renderer_config.rt_test_ray_quality_tier,
        );
    }

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

    // --list-cells [FILTER]: print every interior cell (`--cell`) and
    // worldspace (`--wrld`) in the current plugin set, then exit. No
    // window, no Vulkan — just the ESM parse the cell loader would do
    // anyway. `--game <key>` expansion above already supplied `--esm`,
    // and repeatable `--master <path>` is honoured, so DLC interiors
    // list under the same load order that would load them. The
    // optional positional FILTER is a case-insensitive substring
    // matched against editor ID and display name. See `list_cells`.
    if let Some(idx) = args.iter().position(|a| a == "--list-cells") {
        let esm_path = parse_string_arg(&args, "--esm").ok_or_else(|| {
            anyhow::anyhow!("--list-cells requires --esm <PATH> or --game <KEY> to name a plugin")
        })?;
        // The value after the flag is a filter only when it isn't the
        // next flag (`--list-cells` on its own lists everything).
        let filter = args
            .get(idx + 1)
            .filter(|v| !v.starts_with("--"))
            .map(|s| s.as_str());
        let masters: Vec<&str> = args
            .iter()
            .enumerate()
            .filter_map(|(i, a)| (a == "--master").then(|| args.get(i + 1)).flatten())
            .map(|s| s.as_str())
            .collect();
        let plugin_paths: Vec<&str> = masters
            .into_iter()
            .chain(std::iter::once(esm_path.as_str()))
            .collect();
        return crate::list_cells::run(&plugin_paths, filter);
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
            "--menu",
            "--menu-archive",
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

    let mut app = App::new(debug_mode, &args, renderer_config);
    if let Some(mode) = initial_render_debug_mode {
        app.world
            .resource_mut::<crate::components::RenderDebugControl>()
            .pending_mode = Some(mode);
    }
    app.bench_mode = bench_mode;
    app.bench_frames_target = bench_frames;
    app.bench_hold = bench_hold;
    app.groundcover_bench = groundcover_bench;
    app.groundcover_debug_points = groundcover_debug_points;
    app.groundcover_off = groundcover_off;
    app.screenshot_path = screenshot_path;
    app.bench_camera = bench_camera;
    app.camera_pos_override = camera_pos;
    app.camera_forward_override = camera_forward;
    event_loop.run_app(&mut app)?;

    Ok(())
}

/// #3855 — pins for the two structural invariants [`SOURCES`] carries.
///
/// Neither is visible to the compiler: breaking either leaves the crate
/// building and the scheduler's source-shape tests *green but vacuous*,
/// which is the exact failure this split had to avoid introducing.
///
/// This module is itself part of `SOURCES` — `mod.rs` is the first entry —
/// so it is written to be unable to satisfy or truncate its own assertions.
/// Sentinels are assembled at run time rather than written as literals (a
/// literal here would sit at offset ~0 of the concatenation and truncate
/// every registration away — six tests failed exactly that way, twice,
/// while this split was being made), and the witness scan runs against
/// `SOURCES` with this file's own contribution stripped off the front.
#[cfg(test)]
mod sources_ordering_tests {
    use super::SOURCES;

    /// This file's own text, which is the first entry in `SOURCES`.
    const SELF_SRC: &str = include_str!("mod.rs");

    /// The source-shape modules that truncate `SOURCES` at their own `mod`
    /// declaration. Stored without the `mod ` prefix on purpose.
    const TRUNCATING_TEST_MODULES: &[&str] = &[
        "fragment_activation_order_tests",
        "scheduler_timings_gate_tests",
        "system_access_declaration_tests",
    ];

    /// #4088 — every `#[cfg(test)] mod` in `schedule/mod.rs` (the file
    /// deliberately last in `SOURCES`) must be accounted for as EITHER a
    /// truncation sentinel above, OR named here with why it doesn't need to
    /// be one — so a new 5th module forces a human decision instead of
    /// silently landing in neither list. Nothing checked
    /// `TRUNCATING_TEST_MODULES` for completeness before this: the list was
    /// hand-maintained, and it was already one module short in practice
    /// (see below) with nothing catching it.
    const NON_SENTINEL_TEST_MODULES: &[(&str, &str)] = &[(
        "scheduler_access_report_tests",
        "calls build_scheduler() directly and asserts on its runtime \
         access_report() — no include_str!/text-scan of its own, so \
         nothing about its body could be mistaken for a production \
         registration by another scan. #4088: this module already existed \
         without being in TRUNCATING_TEST_MODULES or anywhere else — this \
         entry documents that as deliberate rather than leaving it \
         silently unaccounted for.",
    )];

    /// One distinctive registration per production file, so each is
    /// independently proven present rather than the concatenation merely
    /// being non-empty.
    const PER_FILE_WITNESS: &[(&str, &str)] = &[
        ("world.rs", "world.insert_resource(DeltaTime(0.0));"),
        ("schedule/early.rs", "timer_tick_system"),
        ("schedule/update.rs", "combat_damage_system"),
        (
            "schedule/post_update.rs",
            "make_world_bound_propagation_system",
        ),
        ("schedule/physics.rs", "physics_sync_system"),
        ("schedule/late.rs", "extension_cell_load_dispatch_system"),
        ("registries.rs", "known_conflict_count()"),
        ("cli.rs", "fn expand_game_profile_args"),
    ];

    /// Every source-shape module truncates `SOURCES` at its own `mod`
    /// declaration so its own mentions of a system name cannot be what a
    /// scan finds. That convention is only sound while every production
    /// registration appears *before* the earliest such declaration — the
    /// property this asserts, per sentinel and per file.
    #[test]
    fn every_production_file_precedes_the_first_test_module() {
        let others = SOURCES.strip_prefix(SELF_SRC).expect(
            "mod.rs must be the FIRST entry in the SOURCES concat! — the witness scan \
             below strips it so this file's own literals cannot satisfy the assertions",
        );
        for module in TRUNCATING_TEST_MODULES {
            let sentinel = format!("mod {module}");
            let prefix = others
                .split(sentinel.as_str())
                .next()
                .expect("split always yields a first segment");
            assert!(
                prefix.len() < others.len(),
                "`{sentinel}` is no longer present in SOURCES — a source-shape module \
                 was renamed or removed without updating this pin, and its own \
                 truncation is now a silent no-op"
            );
            for (file, witness) in PER_FILE_WITNESS {
                assert!(
                    prefix.contains(witness),
                    "boot/{file} must appear in SOURCES before `{sentinel}`, but its \
                     witness `{witness}` is not in the truncated prefix. Reorder the \
                     `concat!` so every production file precedes the test modules that \
                     scan it — otherwise those scans silently assert on nothing."
                );
            }
        }
    }

    /// #4088 — `TRUNCATING_TEST_MODULES` was hand-maintained with no check
    /// that it names every `#[cfg(test)] mod` in `schedule/mod.rs` (the file
    /// deliberately last in `SOURCES`, sharing its own four test modules).
    /// It was already incomplete in practice:
    /// `scheduler_access_report_tests` exists in that file and was in
    /// neither `TRUNCATING_TEST_MODULES` nor any exemption list — this test
    /// would have failed had it existed sooner. A fifth module landing in
    /// either state (sentinel needed but unlisted, or genuinely exempt but
    /// undocumented) now fails loudly instead of silently truncating on a
    /// no-op or asserting on the wrong text.
    #[test]
    fn every_schedule_mod_test_module_is_a_sentinel_or_a_documented_exemption() {
        const SCHEDULE_MOD_SRC: &str = include_str!("schedule/mod.rs");
        let mut found = Vec::new();
        let mut rest = SCHEDULE_MOD_SRC;
        while let Some(at) = rest.find("#[cfg(test)]") {
            rest = &rest[at + "#[cfg(test)]".len()..];
            let after_attr = rest.trim_start();
            let Some(after_mod) = after_attr.strip_prefix("mod ") else {
                continue;
            };
            let name_end = after_mod
                .find(|c: char| !(c.is_alphanumeric() || c == '_'))
                .expect("a mod name is followed by whitespace or `{`");
            found.push(after_mod[..name_end].to_string());
            rest = &after_mod[name_end..];
        }
        assert!(
            !found.is_empty(),
            "sanity: schedule/mod.rs must still carry #[cfg(test)] modules"
        );
        for name in &found {
            let is_sentinel = TRUNCATING_TEST_MODULES.contains(&name.as_str());
            let exemption = NON_SENTINEL_TEST_MODULES
                .iter()
                .find(|(module, _)| *module == name.as_str());
            assert!(
                is_sentinel || exemption.is_some(),
                "schedule/mod.rs's `mod {name}` is in neither TRUNCATING_TEST_MODULES \
                 nor NON_SENTINEL_TEST_MODULES — decide whether its own text could be \
                 mistaken for a production registration by another SOURCES scan, then \
                 add it to whichever list applies (#4088)"
            );
        }
        // The reverse direction: a stale sentinel/exemption entry naming a
        // module that no longer exists is equally worth catching.
        for module in TRUNCATING_TEST_MODULES {
            assert!(
                found.contains(&module.to_string()),
                "TRUNCATING_TEST_MODULES names `{module}`, which no longer exists as a \
                 #[cfg(test)] mod in schedule/mod.rs — remove the stale entry"
            );
        }
        for (module, _) in NON_SENTINEL_TEST_MODULES {
            assert!(
                found.contains(&module.to_string()),
                "NON_SENTINEL_TEST_MODULES names `{module}`, which no longer exists as a \
                 #[cfg(test)] mod in schedule/mod.rs — remove the stale entry"
            );
        }
    }

    /// Every `.rs` file under `boot/` must be in the `concat!`.
    ///
    /// A new file carrying registrations that nobody adds to `SOURCES` is
    /// invisible to the scheduler's source-shape tests: they keep passing
    /// while covering strictly less than they claim to. That is worse than a
    /// red build, so it gets its own gate.
    #[test]
    fn the_concat_list_covers_every_file_in_the_boot_directory() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/boot");
        let mod_rs = std::fs::read_to_string(root.join("mod.rs")).unwrap();
        let (_, list) = mod_rs
            .split_once("SOURCES: &str = concat!(")
            .expect("the SOURCES concat! must exist");
        let list = &list[..list.find(");").expect("concat! must be closed")];

        // #4087 — a hard-coded two-directory list reproduces exactly the
        // hole this gate exists to close the moment a file lands under any
        // THIRD directory (`boot/schedule/extra/foo.rs`, a future
        // `boot/registries/`, …): the gate stays green while covering
        // strictly less than it claims to. Walks the whole `boot/` tree
        // instead, the same directory-stack pattern
        // `crates/renderer/src/vulkan/image.rs`'s sibling completeness gate
        // uses, so a new subdirectory is found rather than silently skipped.
        let mut found = Vec::new();
        let mut stack = vec![root.clone()];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                    continue;
                }
                let rel = path
                    .strip_prefix(&root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                found.push(rel);
            }
        }
        found.sort();
        assert!(!found.is_empty(), "sanity: boot/ must contain .rs files");
        for file in &found {
            assert!(
                list.contains(&format!("include_str!(\"{file}\")")),
                "boot/{file} is not in the SOURCES concat!. Add it — every source-shape \
                 test in this crate reads that string, so an omitted file is coverage \
                 that silently disappears rather than a test that fails."
            );
        }
    }
}
