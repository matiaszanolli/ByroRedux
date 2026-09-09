//! Runtime registry install (#3855, split from `boot.rs`).
//!
//! Runs once after both the world and the schedule exist, and carries the
//! three release assertions that are the construction-time deadlock proof
//! for every same-stage parallel batch (#3111).

use byroredux_core::ecs::{Scheduler, SystemList, World};

use crate::commands::build_command_registry;

/// Phase 3 of construction (#1670) — post-build runtime registries: the
/// scheduler-derived resources (`SystemList`, access report) plus the
/// console-command and save registries. Reads the built `scheduler`
/// immutably; mutates `world`. Extracted verbatim from `App::new`.
pub(crate) fn install_runtime_registries(world: &mut World, scheduler: &Scheduler) {
    // #1394 / #2690 — this access report is the construction-time static
    // deadlock proof for every same-stage parallel batch. With no undeclared
    // systems or unknown pairs, every pair is analyzable; with no known
    // conflict, no pair overlaps on a component/resource lock where either
    // side writes. The batch therefore has no cross-thread blocking edge and
    // cannot form an ABBA cycle. Keep these as release assertions: schedule
    // construction runs once, and a release-only divergence must not ship
    // without the proof. `BYRO_LOCK_ORDER_CHECK` is a dynamic supplement for
    // exercised paths, not a replacement for this exhaustive declared-pair
    // check.
    let report_snapshot = scheduler.access_report();
    assert_eq!(
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
    assert_eq!(
        report_snapshot.known_conflict_count(),
        0,
        "declared access conflict between two parallel same-stage systems \
         — make one side exclusive or split the access (see sys.accesses)"
    );
    assert_eq!(
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
    let mut command_registry = build_command_registry();
    crate::extensions::register_console_commands(world, &mut command_registry);
    world.insert_resource(command_registry);
    // M45 — install the save registry + slot directory so the
    // `save` / `save.info` console commands can operate. Saves live
    // under `<cwd>/saves` (or `BYROREDUX_SAVE_DIR`, #3009); the ring keeps
    // the last 10 quicksaves so a fresh save never immediately clobbers the
    // previous good one.
    world.insert_resource(crate::save_io::build_save_registry());
    world.insert_resource(crate::save_io::SaveState::new(
        crate::save_io::discover_save_dir(),
        10,
    ));
    // M45.1 — deferred live-load slot, drained by `step_save_loads`
    // between frames (the `load` command has only `&World`).
    world.insert_resource(crate::save_io::PendingSaveLoadSlot::default());
    // #3113 — keyboard/menu/SDK save actions enter through one FIFO and
    // execute only after the scheduler has joined its parallel batch.
    world.insert_resource(crate::save_io::PendingPlayerSaveActions::default());
    world.insert_resource(crate::save_io::SaveLoadNotifications::default());
    // M45.1 refinement — player/camera pose, refreshed each frame by
    // `capture_player_pose` and rode along in the snapshot so `load`
    // restores the saved spot instead of the cell's default door.
    world.insert_resource(crate::save_io::PlayerPose::default());
}
