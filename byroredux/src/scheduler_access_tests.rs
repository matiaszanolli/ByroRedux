//! Static source-assertion tests for scheduler construction correctness
//! (AUDIT_CONCURRENCY_2026-07-02, #1785 / #1787 / #1788).
//!
//! Most of these can't be live-tested: the declarations live in
//! `boot::build_scheduler` (split out of `App::new` by #1858 / TD1-003) and
//! the construction order lives inline in `App::new` itself (`main.rs`).
//! Which *types* a declaration names is only visible in source, so each of
//! those tests greps for the exact call the fix added or reordered — a
//! future edit that drops a declaration back out (while the system body
//! still touches it) or reverts a call order fails the build instead of
//! silently regressing the invariant.
//!
//! The aggregate invariants are the exception, and #2138 turned them into a
//! real test: `build_scheduler()` takes no arguments and only registers
//! function pointers, so the whole schedule can be built in-process with no
//! window, device, or loaded world. See
//! `scheduler_access_invariants_hold_on_the_real_schedule` below.

const MAIN_RS: &str = include_str!("main.rs");
const BOOT_RS: &str = include_str!("boot.rs");

/// #1785 / CONC-D3-02 — `apply_color_channels` writes all five
/// `ColorTarget` color-sink storages (Diffuse, Ambient, Specular,
/// Emissive, ShaderColor); the animation system's declaration must
/// claim all five, not just the two it had before the fix.
#[test]
fn animation_declaration_writes_all_five_color_sinks() {
    for ty in [
        "AnimatedDiffuseColor",
        "AnimatedAmbientColor",
        "AnimatedSpecularColor",
        "AnimatedEmissiveColor",
        "AnimatedShaderColor",
    ] {
        let needle = format!(".writes::<byroredux_core::ecs::{ty}>()");
        assert!(
            BOOT_RS.contains(&needle),
            "animation_system declaration is missing `{needle}` — \
             apply_color_channels writes this color sink, see \
             byroredux/src/systems/animation.rs",
        );
    }
}

/// #1787 / CONC-D4-01 — `physics_sync_system`'s body reads `ContactConfig`
/// (register_newcomers) and, behind the opt-in `BYRO_PROFILE_FALLERS`
/// diagnostic, `RenderLayer` / `FormIdComponent` / `PhysicsSourceForm` /
/// `FormIdPool` (all in `dump_awake_fallers`, crates/physics/src/sync.rs).
/// The runtime gate is invisible to the declaration; all five reads must
/// be present regardless.
#[test]
fn physics_sync_declaration_reads_contact_config_and_faller_dump_types() {
    for needle in [
        ".reads::<byroredux_core::ecs::components::RenderLayer>()",
        ".reads::<byroredux_core::ecs::components::FormIdComponent>()",
        ".reads::<byroredux_core::ecs::components::PhysicsSourceForm>()",
        ".reads_resource::<byroredux_core::form_id::FormIdPool>()",
    ] {
        assert!(
            BOOT_RS.contains(needle),
            "physics_sync_system declaration is missing `{needle}` — \
             see crates/physics/src/sync.rs::dump_awake_fallers",
        );
    }
}

/// #1787 / CONC-D4-01 (consolidates CONC-D3-03) — both
/// `physics_sync_system` (register_newcomers) and
/// `player_controller_system` (systems/character.rs) snapshot
/// `ContactConfig` once per tick; both declarations must claim the read.
#[test]
fn contact_config_read_is_declared_on_both_physics_systems() {
    let needle = ".reads_resource::<byroredux_physics::ContactConfig>()";
    let count = BOOT_RS.matches(needle).count();
    assert_eq!(
        count, 2,
        "expected exactly 2 occurrences of `{needle}` (physics_sync_system \
         + player_controller_system), found {count} — see \
         crates/physics/src/sync.rs::register_newcomers and \
         byroredux/src/systems/character.rs",
    );
}

/// #3121 — WATAL buoyancy reads live time/weather/current state, while the
/// animation subtree walk reads `Children`. Keep those reads on the exact
/// parallel-system declarations so the scheduler can reject future conflicts.
#[test]
fn water_and_animation_parallel_accesses_are_complete() {
    let animation = BOOT_RS
        .split("make_animation_system(),")
        .nth(1)
        .and_then(|tail| tail.split("// Translate clip text keys").next())
        .expect("animation access declaration");
    assert!(
        animation.contains(".reads::<byroredux_core::ecs::Children>()"),
        "animation declaration must include the subtree walk's Children read"
    );

    let physics = BOOT_RS
        .split("byroredux_physics::physics_sync_system,")
        .nth(1)
        .and_then(|tail| tail.split("// M28.5 — camera follow").next())
        .expect("physics-sync access declaration");
    for needle in [
        ".reads_resource::<TotalTime>()",
        ".reads_resource::<byroredux_core::ecs::components::groundcover::WindField>()",
        ".reads::<byroredux_core::ecs::components::water::WaterCurrentVolume>()",
    ] {
        assert!(
            physics.contains(needle),
            "physics-sync declaration is missing `{needle}`"
        );
    }
}

/// #1788 / CONC-D4-02 — `debug_server::start` must run before
/// `install_runtime_registries` in `App::new`: the former adds
/// `DebugDrainSystem` to the scheduler via `add_exclusive`, and the
/// latter snapshots `SystemList`/`SchedulerAccessReport` from the
/// scheduler as it stands at that point. Snapshotting first silently
/// dropped the drain system from the `systems` / `sys.accesses` console
/// output on every debug-mode launch — `debug_server::start`'s own doc
/// comment already states this precondition ("Call this after all
/// systems have been added to the scheduler").
#[test]
fn debug_server_start_runs_before_runtime_registries_snapshot() {
    let start_call = "byroredux_debug_server::start(&mut scheduler, debug_port)";
    let snapshot_call = "boot::install_runtime_registries(&mut world, &scheduler);";

    let start_pos = MAIN_RS
        .find(start_call)
        .unwrap_or_else(|| panic!("`{start_call}` not found in main.rs — App::new changed shape"));
    let snapshot_pos = MAIN_RS.find(snapshot_call).unwrap_or_else(|| {
        panic!("`{snapshot_call}` not found in main.rs — App::new changed shape")
    });

    assert!(
        start_pos < snapshot_pos,
        "debug_server::start (byte {start_pos}) must appear before \
         install_runtime_registries (byte {snapshot_pos}) in App::new, or \
         DebugDrainSystem is silently omitted from `systems`/`sys.accesses`",
    );
}

/// #2138 / CONC-D4-NEW-02 — the three scheduler-access invariants (#1394,
/// #1602) as a real test rather than a `debug_assert` nobody runs.
///
/// The guards live in `boot::install_runtime_registries`, whose sole caller
/// is `App::new`. `cargo test` never reaches them, and the one CI job that
/// does boot the engine used to discard the process exit code — so a tripped
/// guard went green on both. That made the primary regression pin for this
/// whole dimension enforced by nothing; a future `add_to()` or a new
/// conflicting pair (the exact #1601 shape) could reach `main` unnoticed.
///
/// Building the schedule here is cheap and dependency-free: `build_scheduler`
/// registers function pointers and access declarations only, touching no
/// Vulkan device, window, or world. That makes these three counts assertable
/// in `cargo test --workspace`, independent of the `vulkan-validation` job.
#[test]
fn scheduler_access_invariants_hold_on_the_real_schedule() {
    let report = crate::boot::build_scheduler().access_report();

    // Non-vacuity: all three assertions below are "count == 0" shaped, so an
    // empty report would pass every one of them. Pin a floor on the system
    // count so a refactor that stops populating the schedule fails loudly
    // instead of turning this test into a no-op.
    assert!(
        report.system_count() > 20,
        "access report holds only {} systems — build_scheduler stopped \
         populating the schedule, which would make the invariants below \
         vacuously true",
        report.system_count(),
    );

    // #2393 / ECS-D5B-02 — `system_count` counts exclusives too, so it is
    // satisfied by ~46 systems even if every parallel entry were demoted
    // to `add_exclusive` tomorrow, which would leave the conflict
    // assertions below analyzing nothing. M27's resolution pattern is
    // monotone demotion (every conflict so far was closed by making one
    // side exclusive) and the boot guard never fails on an empty parallel
    // batch, so erosion is the path of least resistance. Floor the two
    // quantities that actually carry the invariant's meaning: how many
    // systems can be paired, and how many pairs were examined. Deliberate
    // demotion is still allowed — it just has to be an explicit edit here.
    assert!(
        report.parallel_system_count() >= 10,
        "only {} parallel systems remain (was 10) — a demotion to \
         add_exclusive shrank the analyzable population. If deliberate, \
         lower this floor in the same commit; otherwise the conflict \
         assertions below are quietly analyzing less than they used to \
         (#2393)",
        report.parallel_system_count(),
    );
    assert!(
        report.analyzed_pair_count() >= 9,
        "the analyzer examined only {} same-stage pairs (was 9: Early 3 + \
         Late 6) — three of five stages already hold a single parallel \
         system and analyze nothing, so this is the invariant's real \
         coverage measure (#2393)",
        report.analyzed_pair_count(),
    );

    let undeclared: Vec<&str> = report
        .stages
        .iter()
        .flat_map(|s| s.systems.iter())
        .filter(|row| !row.is_exclusive && row.declared.is_none())
        .map(|row| row.name.as_str())
        .collect();
    assert!(
        undeclared.is_empty(),
        "undeclared parallel system(s) {undeclared:?} — use \
         add_to_with_access instead of add_to (#1394)",
    );

    let mut conflicts = Vec::new();
    let mut unknown = Vec::new();
    for stage in &report.stages {
        for row in &stage.conflicts {
            let pair = format!("{:?}: {} <-> {}", stage.stage, row.left, row.right);
            match row.conflict {
                byroredux_core::ecs::AccessConflict::Conflict { .. } => conflicts.push(pair),
                byroredux_core::ecs::AccessConflict::Unknown { .. } => unknown.push(pair),
                byroredux_core::ecs::AccessConflict::None => {}
            }
        }
    }
    assert!(
        conflicts.is_empty(),
        "declared access conflict(s) between parallel same-stage systems \
         {conflicts:?} — make one side exclusive or split the access \
         (run `sys.accesses`) (#1602)",
    );
    assert!(
        unknown.is_empty(),
        "unknown access pair(s) {unknown:?} — the analyzer cannot classify \
         these, so they are indistinguishable from a real conflict (#1602)",
    );
}

const CI_YML: &str = include_str!("../../.github/workflows/ci.yml");

/// The `vulkan-validation` job's slice of the workflow — from its job key to
/// the end of the file (it is the last job). Used by the two tests below.
fn vulkan_validation_job() -> &'static str {
    let start = CI_YML
        .find("\n  vulkan-validation:")
        .expect("the vulkan-validation job disappeared from .github/workflows/ci.yml");
    &CI_YML[start..]
}

/// #2137 / CONC-D4-NEW-01 — the ABBA lock-order detector must be enabled on
/// the one CI job that boots the real engine.
///
/// `lock-order-check` runs `cargo test` with the detector on, but those are
/// single-threaded hand-built `World`s. `vulkan-validation` is the only job
/// where rayon dispatches the real parallel batch across worker threads
/// against a real loaded world — the exact workload the cross-thread graph
/// was built for — and it used to be the one place the detector was off.
/// `ENABLED` is a `LazyLock<AtomicBool>` seeded from the environment at first
/// touch, so without the env var the detector is compiled in but inert.
#[test]
fn vulkan_validation_job_enables_the_lock_order_detector() {
    assert!(
        vulkan_validation_job().contains("BYRO_LOCK_ORDER_CHECK: 1"),
        "the vulkan-validation job no longer sets BYRO_LOCK_ORDER_CHECK — the \
         only CI job that boots the engine runs with the detector inert \
         (#2137); see crates/core/src/ecs/lock_tracker.rs",
    );
}

/// #2138 / CONC-D4-NEW-02 — the `vulkan-validation` job must not swallow a
/// panic.
///
/// The step's sole failure predicate used to be a `[Vulkan]` substring match,
/// with `|| true` discarding the exit code. Panic text carries no `[Vulkan]`
/// marker, so a tripped assertion — the `BYRO_LOCK_ORDER_CHECK` detector
/// enabled by the test above, or anything else that fires only under a live
/// engine — went green. The companion
/// `scheduler_access_invariants_hold_on_the_real_schedule` covers the
/// #1394/#1602 guards specifically; this pins the general case.
#[test]
fn vulkan_validation_job_fails_on_a_panic() {
    let job = vulkan_validation_job();
    assert!(
        job.contains("grep -qF 'panicked at'"),
        "the vulkan-validation job no longer greps for 'panicked at' — a \
         tripped assertion in the live 5-frame run would go green again \
         (#2138)",
    );
    assert!(
        !job.contains("--bench-frames 5 2>&1 || true"),
        "the vulkan-validation job went back to discarding the bench exit \
         code with `|| true` (#2138)",
    );
}

/// #2676 / CONC-D3-NEW-02 — `camera_follow_system`'s first statement
/// reads the `PlayerMode` resource as an early-out gate, but its
/// `Access` declaration omitted it. `Stage::Late` is the engine's
/// largest parallel batch, so the analyzer's `known_conflict_count()`
/// — the invariant that keeps cross-thread ABBA structurally
/// unreachable among parallel systems — was being computed from an
/// incomplete declaration. Scoped to this registration's own argument
/// list, because `player_controller_system` declares the same read.
#[test]
fn camera_follow_declaration_reads_player_mode() {
    let reg_start = BOOT_RS
        .find("crate::systems::camera_follow_system,")
        .expect("camera_follow_system must still be registered in build_scheduler");
    // The declaration ends at the first `);` closing `add_to_with_access`.
    let reg_end = BOOT_RS[reg_start..]
        .find("\n    );")
        .map(|i| reg_start + i)
        .expect("the camera_follow_system registration must close");
    let decl = &BOOT_RS[reg_start..reg_end];
    assert!(
        decl.contains(".reads_resource::<crate::systems::PlayerMode>()"),
        "camera_follow_system's Access is missing \
         `.reads_resource::<crate::systems::PlayerMode>()` — its body gates \
         on PlayerMode (byroredux/src/systems/character.rs), and an \
         incomplete Late-stage declaration makes the zero-conflict \
         invariant unsound (#2676)",
    );
}

/// #2389 / ECS-D5-01 — both telemetry systems in the `Stage::Late`
/// parallel batch read resources their declaration omitted:
/// `log_stats_system` reads `SkinCoverageStats` + `CpuFrameTimings`
/// behind its `want_breakdown` gate (`systems/debug.rs`), and
/// `metrics_sample_system` reads `CpuFrameTimings` +
/// `SchedulerSystemTimings` on a sample tick (`systems/metrics.rs`).
/// Runtime gates are invisible to the analyzer, so an incomplete
/// declaration makes `AccessConflict::None` a claim the analyzer never
/// proved. Scoped to each registration's own argument list.
#[test]
fn late_telemetry_declarations_read_all_their_resources() {
    // The needle carries the registration's own indentation + the
    // following `Access::new()` so it can't match the `use
    // crate::systems::{…}` import list at the top of boot.rs.
    for (system, needles) in [
        (
            "        log_stats_system,\n        Access::new()",
            [
                ".reads_resource::<SkinCoverageStats>()",
                ".reads_resource::<byroredux_core::ecs::CpuFrameTimings>()",
            ],
        ),
        (
            "        metrics_sample_system,\n        Access::new()",
            [
                ".reads_resource::<byroredux_core::ecs::CpuFrameTimings>()",
                ".reads_resource::<byroredux_core::ecs::SchedulerSystemTimings>()",
            ],
        ),
    ] {
        let reg_start = BOOT_RS
            .find(system)
            .unwrap_or_else(|| panic!("{system} must still be registered in build_scheduler"));
        let reg_end = BOOT_RS[reg_start..]
            .find("\n    );")
            .map(|i| reg_start + i)
            .unwrap_or_else(|| panic!("{system}'s registration must close"));
        let decl = &BOOT_RS[reg_start..reg_end];
        for needle in needles {
            assert!(
                decl.contains(needle),
                "{system} declaration is missing `{needle}` — its body reads \
                 that resource behind a runtime gate the analyzer cannot see \
                 (#2389)",
            );
        }
    }
}

/// #2391 / ECS-D5B-03 — `add_exclusive_with_access` (added by #1236 as
/// the declaration channel for closures and bare `fn` exclusives) had
/// zero production call sites, so all 43 exclusive registrations
/// reported blank `sys.accesses` rows — including the handful whose
/// *only* safety argument is the exclusive scheduling itself
/// (`pool_regen_tick_system`'s 3-deep hold stack, #2153; the
/// cinematic/quest-stage lock-order inversion, #2269; the PostUpdate
/// `GlobalTransform` ordering chain). Those now declare.
///
/// Matched on a name substring because `System::name()` returns
/// `type_name::<Self>()`, which for the `make_*` closure systems is a
/// synthesised `{{closure}}` path rather than the factory's name.
#[test]
fn contract_bearing_exclusives_declare_their_access() {
    let report = crate::boot::build_scheduler().access_report();
    let rows: Vec<(&str, bool, bool)> = report
        .stages
        .iter()
        .flat_map(|s| s.systems.iter())
        .map(|row| (row.name.as_str(), row.is_exclusive, row.declared.is_some()))
        .collect();

    for needle in [
        "pool_regen_tick_system",
        "cinematic_animation_event_system",
        "submersion_system",
        "billboard",
        "bounds",
    ] {
        let matching: Vec<&(&str, bool, bool)> = rows
            .iter()
            .filter(|(name, is_exclusive, _)| *is_exclusive && name.contains(needle))
            .collect();
        assert!(
            !matching.is_empty(),
            "no exclusive system whose name contains `{needle}` — the \
             registration moved or was renamed (#2391)",
        );
        assert!(
            matching.iter().all(|(_, _, declared)| *declared),
            "exclusive system(s) matching `{needle}` still report a blank \
             access row: {:?} — declare them via add_exclusive_with_access \
             so the disputed types are visible in `sys.accesses` (#2391)",
            matching,
        );
    }
}

/// #3180 — `camera_follow_system`'s comment claims it runs before both
/// `audio_system` **and** `submersion_system`. That was only half true:
/// `submersion_system` was registered in `Stage::PostUpdate`, an earlier stage
/// entirely, so in player / third-person mode it read the previous frame's
/// camera pose (`camera_follow_system` authors the pose in `Stage::Late`).
///
/// This pins the ordering the comment asserts, on the real schedule, so the
/// claim cannot re-rot into aspiration. Within a stage the access report lists
/// the parallel batch first and then exclusives in registration order, which
/// is also the execution order.
#[test]
fn submersion_runs_after_camera_follow_and_before_water_audio() {
    use byroredux_core::ecs::Stage;

    let report = crate::boot::build_scheduler().access_report();
    let late = report
        .stages
        .iter()
        .find(|s| s.stage == Stage::Late)
        .expect("Stage::Late must exist in the schedule");

    let index_of = |needle: &str| -> usize {
        late.systems
            .iter()
            .position(|row| row.name.contains(needle))
            .unwrap_or_else(|| {
                panic!(
                    "no Stage::Late system matching `{needle}` — the \
                     registration moved stage or was renamed (#3180). \
                     Late systems: {:?}",
                    late.systems.iter().map(|r| &r.name).collect::<Vec<_>>()
                )
            })
    };

    // Fully-qualified needles: `audio_system` is a suffix of
    // `water_audio_system`, so a `contains` on the bare name matches both.
    let camera_follow = index_of("systems::character::camera_follow_system");
    let submersion = index_of("systems::water::submersion_system");
    let water_audio = index_of("systems::audio::water_audio_system");
    let audio = index_of("byroredux_audio::audio_system");
    let ragdoll = index_of("ragdoll::ragdoll_writeback_system");

    assert!(
        !late.systems[camera_follow].is_exclusive,
        "camera_follow_system is no longer in the Late parallel batch — the \
         'parallel batch completes before exclusives' argument that makes \
         this ordering structural no longer applies (#3180)"
    );
    assert!(
        late.systems[submersion].is_exclusive,
        "submersion_system must stay a Late exclusive so it sequences after \
         the parallel batch that authors the camera pose (#3180)"
    );
    assert!(
        ragdoll < submersion,
        "the documented Late exclusive order is ragdoll -> submersion -> \
         water_damage -> water_interaction -> water_audio -> audio_system -> \
         event_cleanup; submersion ({submersion}) must follow \
         ragdoll_writeback ({ragdoll}) (#3180)"
    );
    assert!(
        camera_follow < submersion,
        "submersion_system ({submersion}) must run AFTER camera_follow_system \
         ({camera_follow}) — otherwise it reads a stale camera pose in \
         player mode and the underwater low-pass lags a frame (#3180)"
    );
    assert!(
        submersion < water_audio,
        "submersion_system ({submersion}) must run BEFORE water_audio_system \
         ({water_audio}), which consumes the SubmersionState and the \
         Splash/Ripple markers it writes (#3180)"
    );
    assert!(
        water_audio < audio,
        "water_audio_system ({water_audio}) must run BEFORE audio_system \
         ({audio}) — it sets AudioWorld::underwater that the filter pass reads"
    );

    // The regression itself: an earlier-stage registration would silently
    // restore the one-frame lag while every assertion above still passed.
    for stage in &report.stages {
        if stage.stage == Stage::Late {
            continue;
        }
        assert!(
            !stage
                .systems
                .iter()
                .any(|r| r.name.contains("systems::water::submersion_system")),
            "submersion_system is registered in {:?} as well as Stage::Late — \
             an earlier-stage copy reads the previous frame's camera pose \
             (#3180)",
            stage.stage,
        );
    }
}
