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
