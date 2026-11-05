//! Scheduler construction (#3855, split from `boot.rs`).
//!
//! #3739 split `build_scheduler` into five per-stage `register_*_systems`
//! functions; this module is that split carried to file level, one file per
//! stage. `build_scheduler` stays the 18-line orchestrator it became, and
//! remains the authority for *which stage does X run in*.

mod early;
mod late;
mod physics;
mod post_update;
mod update;

use early::register_early_systems;
use late::register_late_systems;
use physics::register_physics_systems;
use post_update::register_post_update_systems;
use update::register_update_systems;

use byroredux_core::ecs::Scheduler;

/// `BYRO_NO_AI_LOCOMOTION=1` is the single AI-motion kill switch (M42.10).
/// Read once here so every mover — the PostUpdate locomotion block and
/// the Update-stage force-greet walk step (#5430) — shares one definition.
fn ai_locomotion_enabled() -> bool {
    std::env::var_os("BYRO_NO_AI_LOCOMOTION").is_none()
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
    register_early_systems(&mut scheduler);
    register_update_systems(&mut scheduler);
    register_post_update_systems(&mut scheduler);
    register_physics_systems(&mut scheduler);
    register_late_systems(&mut scheduler);
    scheduler
}

/// Run order of the built schedule, for the ordering guards in this file (#6684).
///
/// `build_scheduler()` registers function pointers and access declarations only,
/// so it needs no window, device or world, and `access_report()` lists what was
/// really registered. Reading that instead of the registration text means moving
/// a registration between files cannot break a guard or leave it checking nothing.
#[cfg(test)]
mod schedule_order {
    use byroredux_core::ecs::{AccessReport, Stage, StageReport};

    /// Where one system sits in the built schedule.
    #[derive(Clone, Copy, Debug)]
    pub(super) struct SchedulePosition {
        stage: Stage,
        /// Position in the stage's report rows: the parallel batch, then the
        /// exclusives in the order they run.
        index: usize,
        exclusive: bool,
    }

    impl SchedulePosition {
        /// Whether this system finishes before `later` starts. Stages run in
        /// order, and within a stage the parallel batch runs before the
        /// exclusives. Two parallel systems in one stage overlap, so neither runs
        /// before the other whatever order they were registered in.
        pub(super) fn runs_before(self, later: Self) -> bool {
            match self.stage.cmp(&later.stage) {
                std::cmp::Ordering::Less => true,
                std::cmp::Ordering::Greater => false,
                std::cmp::Ordering::Equal => later.exclusive && self.index < later.index,
            }
        }
    }

    /// A system's own name, without its module path or a `make_*` constructor's
    /// `{{closure}}` suffix. Guards match this exactly, because a substring match
    /// on `audio_system` would also find `water_audio_system`.
    pub(super) fn bare_name(full: &str) -> &str {
        let path = full.strip_suffix("::{{closure}}").unwrap_or(full);
        path.rsplit("::").next().unwrap_or(path)
    }

    pub(super) struct BuiltSchedule(AccessReport);

    impl BuiltSchedule {
        pub(super) fn new() -> Self {
            Self(super::build_scheduler().access_report())
        }

        /// The position of the one system named `system`. A name that is missing
        /// or ambiguous panics rather than skipping the check, so a rename cannot
        /// leave a guard green while checking nothing.
        pub(super) fn position(&self, system: &str) -> SchedulePosition {
            let mut found = self.0.stages.iter().flat_map(|stage| {
                stage
                    .systems
                    .iter()
                    .enumerate()
                    .filter(|(_, row)| bare_name(&row.name) == system)
                    .map(|(index, row)| SchedulePosition {
                        stage: stage.stage,
                        index,
                        exclusive: row.is_exclusive,
                    })
            });
            let position = found.next().unwrap_or_else(|| {
                panic!(
                    "`{system}` is not in the built schedule: it was renamed, removed, or gated \
                     off by the environment (BYRO_NO_AI_LOCOMOTION drops the AI movers)"
                )
            });
            assert!(
                found.next().is_none(),
                "more than one system in the built schedule is named `{system}`"
            );
            position
        }

        pub(super) fn stage(&self, stage: Stage) -> &StageReport {
            self.0
                .stages
                .iter()
                .find(|report| report.stage == stage)
                .unwrap_or_else(|| panic!("the built schedule has no {stage:?} stage"))
        }
    }

    /// The rule no reorder of today's schedule can exercise, because every
    /// system these guards name is exclusive: parallel systems have no order.
    #[test]
    fn runs_before_follows_stages_and_exclusives_not_parallel_registration_order() {
        let at = |stage, index, exclusive| SchedulePosition {
            stage,
            index,
            exclusive,
        };
        let parallel_a = at(Stage::Late, 0, false);
        let parallel_b = at(Stage::Late, 1, false);
        let exclusive_a = at(Stage::Late, 2, true);
        let exclusive_b = at(Stage::Late, 3, true);

        assert!(!parallel_a.runs_before(parallel_b) && !parallel_b.runs_before(parallel_a));
        assert!(parallel_b.runs_before(exclusive_a) && !exclusive_a.runs_before(parallel_b));
        assert!(exclusive_a.runs_before(exclusive_b) && !exclusive_b.runs_before(exclusive_a));

        let earlier_stage = at(Stage::Update, 9, true);
        assert!(earlier_stage.runs_before(parallel_a) && !parallel_a.runs_before(earlier_stage));
    }

    #[test]
    fn bare_name_drops_the_module_path_and_closure_suffix() {
        assert_eq!(
            bare_name("byroredux::systems::wander::make_wander_system::{{closure}}"),
            "make_wander_system"
        );
        assert_eq!(
            bare_name("byroredux_scripting::cleanup::event_cleanup_system"),
            "event_cleanup_system"
        );
    }

    #[test]
    #[should_panic(expected = "is not in the built schedule")]
    fn position_panics_on_a_name_the_schedule_does_not_hold() {
        BuiltSchedule::new().position("audio");
    }
}

#[cfg(test)]
mod fragment_activation_order_tests {
    //! SCR-D6-NEW11-01 / #2654 — `quest_fragment_dispatch` is the last
    //! producer of `ActivateEvent` in `Stage::Update`, but most of the
    //! consumers are scheduled before it (and it cannot simply move
    //! earlier: it consumes the `QuestStageAdvanced` markers
    //! `quest_advance_dispatch` emits). Fragment activations are therefore
    //! queued and flushed at the head of the next frame — which only works
    //! while `fragment_activation_flush_system` stays registered ahead of
    //! every consumer.
    //!
    //! This module's guards read the built schedule through [`schedule_order`],
    //! not the registration text.

    use super::schedule_order::{bare_name, BuiltSchedule};
    use byroredux_core::ecs::Stage;

    #[test]
    fn activation_flush_is_scheduled_before_every_activate_event_consumer() {
        let schedule = BuiltSchedule::new();
        let flush = schedule.position("fragment_activation_flush_system");
        // #4712 / #4116 — `container_loot_system` and
        // `mg07_on_activate_dispatch` are real Update-stage consumers too;
        // the former was registered ahead of the flush, so a scripted
        // container/pickup `Activate()` never looted.
        for consumer in [
            "container_loot_system",
            "rumble_on_activate_dispatch",
            "quest_advance_dispatch",
            "two_state_activator_system",
            "mg07_on_activate_dispatch",
            "reference_script_event_system",
        ] {
            assert!(
                flush.runs_before(schedule.position(consumer)),
                "fragment_activation_flush_system must be scheduled before `{consumer}`, or \
                 fragment `<Ref>.Activate()` silently no-ops again (#2654)"
            );
        }

        // And the producer still has to run after the advance that feeds it.
        assert!(
            schedule
                .position("quest_advance_dispatch")
                .runs_before(schedule.position("quest_fragment_dispatch")),
            "quest_fragment_dispatch consumes QuestStageAdvanced and must stay after \
             quest_advance_dispatch"
        );
    }

    /// `HitEvent` is a one-frame marker (`event_cleanup_system` drains it in
    /// Late), so a consumer registered ahead of a producer never sees that
    /// producer's hits: the `Weapon.Fire` projectiles and the `ProcessTrapHit`
    /// flush used to land after `combat_damage_system`, and a dart trap's
    /// darts hurt nobody.
    #[test]
    fn hit_event_consumers_run_after_every_producer() {
        let schedule = BuiltSchedule::new();
        for consumer in [
            "combat_damage_system",
            "hit_event_system",
            // #5991 — a consumer of the marker too: its OnHit delivery
            "reference_script_hit_system",
            // #6293 — the ObScript `OnHit` / `OnHitWith` consumer, split out of
            // `obscript_object_system` so it lands after the producers
            "obscript_hit_system",
        ] {
            for producer in [
                // the VM effect flush (`ProcessTrapHit`) queues inside the script systems,
                // and this system lands the queue as `HitEvent`s (#5872)
                "pending_trap_hit_flush_system",
                "reference_script_event_system",
                "alias_script_event_system",
                "projectile_flight_system",
                "combat_input_system",
                "make_npc_combat_ai_system",
                // #6580 — a mine blast hits every actor in its radius (M43 placed mines)
                "mine_system",
            ] {
                assert!(
                    schedule
                        .position(producer)
                        .runs_before(schedule.position(consumer)),
                    "`{consumer}` must run after the `HitEvent` producer `{producer}`"
                );
            }
        }
    }

    /// #5872 — the queue is flushed after the script systems that fill it.
    #[test]
    fn trap_hit_flush_runs_after_the_script_systems_that_queue_into_it() {
        let schedule = BuiltSchedule::new();
        let flush = schedule.position("pending_trap_hit_flush_system");
        for producer in [
            "reference_script_event_system",
            "alias_script_event_system",
            "quest_fragment_dispatch",
        ] {
            assert!(
                schedule.position(producer).runs_before(flush),
                "`{producer}` must run before the trap-hit flush"
            );
        }
    }

    #[test]
    fn extension_event_adapters_run_before_transient_cleanup() {
        const CLEANUP: &str = "event_cleanup_system";
        const SETTING_APPLY: &str = "extension_setting_write_apply_system";
        const CUSTOM: &str = "extension_custom_event_dispatch_system";
        const CATALOG: &str = "extension_content_catalog_sync_system";
        const INPUT_BINDINGS: &str = "extension_input_bindings_sync_system";
        const LEGACY: &str = "legacy_obscript_load_order_system";
        const PAPYRUS: &str = "papyrus_provider_system";
        const ACTIVATION: &str = "extension_activation_dispatch_system";

        let schedule = BuiltSchedule::new();
        for (earlier, later) in [
            ("extension_engine_settings_sync_system", CUSTOM),
            (CATALOG, CUSTOM),
            (CATALOG, INPUT_BINDINGS),
            (INPUT_BINDINGS, LEGACY),
            (CATALOG, LEGACY),
            (LEGACY, PAPYRUS),
            (LEGACY, CLEANUP),
            (PAPYRUS, CLEANUP),
            (CUSTOM, ACTIVATION),
            (CUSTOM, CLEANUP),
            (ACTIVATION, CLEANUP),
            ("extension_cell_load_dispatch_system", CLEANUP),
            ("extension_equipment_dispatch_system", CLEANUP),
            ("extension_input_dispatch_system", CLEANUP),
            ("extension_session_dispatch_system", CLEANUP),
            ("extension_hit_dispatch_system", CLEANUP),
            ("extension_update_dispatch_system", SETTING_APPLY),
            (SETTING_APPLY, CLEANUP),
        ] {
            assert!(
                schedule
                    .position(earlier)
                    .runs_before(schedule.position(later)),
                "`{earlier}` must run before `{later}`: extension callbacks must run before \
                 transient marker cleanup"
            );
        }
    }

    /// #3952 — the scene→quest→continuation chain is load-bearing and was
    /// unpinned: `activation_flush_is_scheduled_before_every_activate_event_consumer`
    /// above covers only the flush/`quest_advance` half.
    ///
    /// Each link is a real data dependency, not a stylistic order:
    /// `scene_playback` emits the scene beats that
    /// `scene_fragment_dispatch` turns into fragments;
    /// `quest_fragment_dispatch` consumes the `QuestStageAdvanced` those
    /// produce; and `fragment_continuation` resumes tails that the two
    /// dispatchers suspend at a provider barrier. Reorder any pair and
    /// the downstream system reads an empty queue for a frame — silent,
    /// and invisible to every other test. #3739's 750-line move and
    /// #3855's file split are exactly the edit class that does this
    /// without anyone noticing.
    #[test]
    fn scene_and_fragment_dispatch_chain_stays_in_dependency_order() {
        let schedule = BuiltSchedule::new();
        let scene_playback = schedule.position("scene_playback_system");
        let scene_fragment = schedule.position("scene_fragment_dispatch_system");
        let quest_fragment = schedule.position("quest_fragment_dispatch");
        let continuation = schedule.position("fragment_continuation_system");

        assert!(
            scene_playback.runs_before(scene_fragment),
            "scene_fragment_dispatch consumes what scene_playback emits"
        );
        assert!(
            scene_fragment.runs_before(quest_fragment),
            "quest_fragment_dispatch consumes the QuestStageAdvanced markers \
             scene_fragment_dispatch can produce"
        );
        assert!(
            quest_fragment.runs_before(continuation),
            "fragment_continuation resumes tails suspended by the dispatchers, \
             so it must run after both of them in the same frame"
        );
    }

    /// #3952 — `event_cleanup_system` drains the transient marker
    /// components every other system keys on, so it must be the last
    /// `Stage::Late` exclusive registered. The extension test above pins
    /// it against a hand-listed set of consumers; this pins it against
    /// *every* Late exclusive, including ones added later that nobody
    /// thinks to add to that list.
    #[test]
    fn transient_cleanup_is_the_last_late_exclusive() {
        let schedule = BuiltSchedule::new();
        let exclusives: Vec<&str> = schedule
            .stage(Stage::Late)
            .systems
            .iter()
            .filter(|row| row.is_exclusive)
            .map(|row| bare_name(&row.name))
            .collect();
        assert!(
            exclusives.len() > 5,
            "expected to inspect the Late exclusives, found {exclusives:?}"
        );
        assert_eq!(
            exclusives.last().copied(),
            Some("event_cleanup_system"),
            "event_cleanup_system must be the last Late exclusive, or the markers it drains are \
             gone before the systems after it read them: {exclusives:?}"
        );
    }
}

#[cfg(test)]
mod ambient_locomotion_default_on_tests {
    //! M42.10 — ambient AI locomotion (sandbox seat + the six procedures +
    //! walk-cycle playback) is registered unconditionally with a single
    //! `BYRO_NO_AI_LOCOMOTION` kill-switch. The seven per-procedure opt-in
    //! variables (`BYRO_SANDBOX_SIT` / `BYRO_WANDER` / `BYRO_TRAVEL` /
    //! `BYRO_FOLLOW` / `BYRO_ESCORT` / `BYRO_GUARD` / `BYRO_PATROL`) are
    //! gone: a stale reference would silently reintroduce a partial gate,
    //! and dropping the kill-switch itself would leave no escape hatch when
    //! a content regression needs isolating from AI motion. Walk playback
    //! must also register *after* the six movers — it classifies motion
    //! from this frame's final position.

    const BOOT_SRC: &str = crate::boot::SOURCES;

    #[test]
    fn locomotion_registers_behind_the_single_kill_switch() {
        let setup = BOOT_SRC
            .split("mod ambient_locomotion_default_on_tests")
            .next()
            .expect("split always yields a first segment");

        assert!(
            setup.contains("std::env::var_os(\"BYRO_NO_AI_LOCOMOTION\")"),
            "the BYRO_NO_AI_LOCOMOTION kill-switch is missing from \
             boot/schedule/mod.rs (`ai_locomotion_enabled`) — ambient \
             locomotion would have no off switch"
        );
        for stale_gate in [
            "BYRO_SANDBOX_SIT",
            "BYRO_WANDER",
            "BYRO_TRAVEL",
            "BYRO_FOLLOW",
            "BYRO_ESCORT",
            "BYRO_GUARD",
            "BYRO_PATROL",
        ] {
            assert!(
                !setup.contains(stale_gate),
                "{stale_gate} is still read in boot/schedule/ — M42.10 replaced \
                 the per-procedure opt-in gates with the single BYRO_NO_AI_LOCOMOTION \
                 kill-switch; a leftover gate re-splits the rollout"
            );
        }
    }

    /// #4709 — the kill switch is documented as the AI-motion debugging aid,
    /// so the block it gates must hold exactly the nine motion systems.
    ///
    /// Scope boundary (decided on #5430): the switch covers *ambient*
    /// AI-package motion. Combat chase (`npc_combat_ai_system`) and scripted
    /// scene/cinematic motion (`cinematic_horse_route_system`, scene
    /// walks) also call `step_toward` but are deliberately OUTSIDE it —
    /// combat is not ambient AI (#4709), and gating scene motion could hang
    /// scenes waiting on actors that never arrive. The exact-list assertion
    /// below keeps them from drifting into the block unnoticed.
    /// The pin above only checked that the env var is read, which is how
    /// combat feedback ended up inside the block unnoticed: isolating
    /// motion silently muted P2 combat takes and sounds.
    #[test]
    fn locomotion_kill_switch_gates_exactly_the_motion_systems() {
        let setup = BOOT_SRC
            .split("mod ambient_locomotion_default_on_tests")
            .next()
            .expect("split always yields a first segment");
        let open = "if locomotion_enabled {\n";
        let start = setup
            .find(open)
            .expect("the BYRO_NO_AI_LOCOMOTION block is gone from boot/schedule/");
        let body = &setup[start + open.len()..];
        let end = body
            .find("\n    }\n")
            .expect("the locomotion block closes at function-body indentation");
        let (block, after) = (&body[..end], &body[end..]);

        let marker = format!("crate::systems::{}", "make_");
        let mut gated: Vec<&str> = block
            .match_indices(marker.as_str())
            .map(|(at, _)| {
                let name = &block[at + marker.len()..];
                &name[..name.find('(').expect("a system constructor call")]
            })
            .collect();
        // #5430 — `eat_sleep_system` once registered as a bare fn path the
        // scan above could not see, so moving it out of the block failed
        // nothing. #5449 gave it a `make_*` constructor (persistent seat
        // scratch), so the scan covers it like the other movers.
        gated.sort_unstable();
        assert_eq!(
            gated,
            [
                "eat_sleep_system",
                "escort_system",
                "find_package_system",
                "flee_package_system",
                "follow_system",
                "guard_system",
                "npc_walk_animation_system",
                "patrol_system",
                "sandbox_seat_system",
                "travel_system",
                "wander_system",
            ],
            "BYRO_NO_AI_LOCOMOTION must gate exactly the AI-motion systems"
        );
        // #5430 — the force-greet walk step is Update-stage, so it is gated
        // by threading the same switch into its constructor rather than by
        // sitting in the block; pin that it does not go back to ungated.
        assert!(
            setup.contains("make_forcegreet_system(super::ai_locomotion_enabled())"),
            "the force-greet walk step must take the BYRO_NO_AI_LOCOMOTION \
             kill switch (boot/schedule/update/)"
        );
        assert!(
            after.contains(&format!("crate::systems::{}()", "make_combat_feedback_system")),
            "combat feedback must register after (outside) the locomotion \
             block — still after walk_anim, but not behind the motion kill switch"
        );
    }

    /// Unlike the two text pins above, this reads the built schedule: walk
    /// playback's order against the movers holds whichever file registers them.
    /// The movers register only while `BYRO_NO_AI_LOCOMOTION` is unset, so run
    /// this test without it.
    #[test]
    fn walk_animation_registers_after_all_seven_movers() {
        let schedule = super::schedule_order::BuiltSchedule::new();
        let walk_anim = schedule.position("make_npc_walk_animation_system");
        for mover in [
            "make_wander_system",
            "make_travel_system",
            "make_follow_system",
            "make_escort_system",
            "make_guard_system",
            "make_flee_package_system",
            "make_find_package_system",
            "make_patrol_system",
            // #5430 — the Eat/Sleep walk-then-seat mover.
            "make_eat_sleep_system",
        ] {
            assert!(
                schedule.position(mover).runs_before(walk_anim),
                "{mover} must run before npc_walk_animation_system — the \
                 walk classifier reads this frame's post-locomotion position"
            );
        }
    }
}

#[cfg(test)]
mod scheduler_timings_gate_tests {
    //! PERF-D1-01 / #2166 — the per-system wall-time tracker is armed by
    //! the *presence* of `SchedulerSystemTimings` in the world. Inserting
    //! it unconditionally at world setup (as this file did before #2166)
    //! silently defeats the #1647 gate: every registered system then pays
    //! an `Instant::now()` plus a shared-`Mutex` push on every frame of
    //! the shipping binary, for a consumer that samples at ≤ 2 Hz.
    //!
    //! Static source checks rather than a live `setup_world` call — the
    //! full boot path wants a Vulkan device and on-disk game data, which
    //! is out of `cargo test` scope. Mirrors the renderer crate's
    //! `include_str!` convention for pinning a source-level invariant.

    const BOOT_SRC: &str = crate::boot::SOURCES;

    /// The only `SchedulerSystemTimings` insert in `boot/schedule/` must sit
    /// behind the `BYRO_PROFILE` env gate.
    #[test]
    fn scheduler_timings_insert_is_env_gated() {
        // Ignore this test module's own mentions of the type name.
        let setup = BOOT_SRC
            .split("mod scheduler_timings_gate_tests")
            .next()
            .expect("split always yields a first segment");

        let inserts: Vec<&str> = setup
            .lines()
            .filter(|l| l.contains("insert_resource") && l.contains("SchedulerSystemTimings"))
            .collect();
        assert_eq!(
            inserts.len(),
            1,
            "expected exactly one SchedulerSystemTimings insert in boot/schedule/, found {}: {:?}",
            inserts.len(),
            inserts,
        );

        let gate = setup
            .find("if std::env::var_os(\"BYRO_PROFILE\").is_some() {")
            .expect(
                "boot/schedule/ must gate the SchedulerSystemTimings insert on BYRO_PROFILE — an \
                 unconditional insert re-arms the scheduler tracker every frame (#2166)",
            );
        let insert = setup
            .find("insert_resource(byroredux_core::ecs::SchedulerSystemTimings::default())")
            .expect("SchedulerSystemTimings insert not found in boot/schedule/");
        assert!(
            insert > gate,
            "the SchedulerSystemTimings insert must appear inside the BYRO_PROFILE gate, \
             not before it (#2166)"
        );
    }

    /// The lazy arm-on-overlay-open path lives in the winit event handler —
    /// `main.rs` until #2731 split it out, `app_events.rs` since. Without it
    /// a normal (non-`BYRO_PROFILE`) run could never populate the Metrics
    /// panel at all. Pin that the fallback exists.
    #[test]
    fn overlay_open_arms_the_tracker_lazily() {
        const EVENT_SRC: &str = include_str!("../../app_events.rs");
        assert!(
            EVENT_SRC.contains("SchedulerSystemTimings::default()"),
            "app_events.rs must insert SchedulerSystemTimings when the F3 debug overlay \
             first opens — boot/schedule/ no longer does it unconditionally (#2166)"
        );
    }
}

#[cfg(test)]
mod scheduler_access_report_tests {
    //! #3111 / ECS-2026-08-20-01 — `install_runtime_registries`'s
    //! `known_conflict_count() == 0` / `undeclared_parallel_count() == 0`
    //! / `unknown_pair_count() == 0` guards only run inside a live
    //! `App::new()` boot (Vulkan device + on-disk game data), so they were
    //! never exercised by `cargo test` — a regression could sit green in CI
    //! indefinitely. `build_scheduler()` itself needs neither: it's a pure
    //! `() -> Scheduler` builder (extracted verbatim from `App::new` in
    //! #1670), so the same guard checks can run directly here.
    //!
    //! This is the honest-declaration half of #3111's fix, not the
    //! scheduling half — `weather_system` moved to
    //! `add_exclusive_with_access` and `player_controller_system` gained
    //! the `WindField` read declaration in the same change (`boot/schedule/`
    //! `build_scheduler`, see the `#3111` comment there). Without a real
    //! `cargo test` assertion, a future regression that re-declared
    //! `weather_system` as parallel (or dropped the `WindField` read) would
    //! only be caught by a debug build actually booting the engine.
    use super::build_scheduler;

    #[test]
    fn build_scheduler_reports_zero_access_conflicts() {
        let scheduler = build_scheduler();
        let report = scheduler.access_report();
        assert_eq!(
            report.undeclared_parallel_count(),
            0,
            "an undeclared parallel system slipped into the schedule — use \
             add_to_with_access instead of add_to"
        );
        assert_eq!(
            report.known_conflict_count(),
            0,
            "declared access conflict between two parallel same-stage systems \
             — make one side exclusive or split the access (see sys.accesses); \
             this is the guard #3111's WindField race would have tripped had \
             the read been declared without also serializing weather_system"
        );
        assert_eq!(
            report.unknown_pair_count(),
            0,
            "unknown (undeclared) parallel pairing detected — declare both sides' access"
        );
    }
}

/// #3951 / #4064 — a declared system's `Access` must not under-report what
/// its body actually acquires.
///
/// Two populations, for two different reasons.
///
/// **Parallel systems (`add_to_with_access`) — the load-bearing set.**
/// `install_runtime_registries`'s three release assertions are the
/// construction-time deadlock proof for every same-stage parallel batch, and
/// that proof is only as sound as the declarations the analyzer reads. An
/// under-declared parallel system yields `known_conflict_count() == 0` while
/// a real write/write overlap sits in the batch. #4064: the check below used
/// to cover *only* the two exclusives, i.e. exactly the population where a
/// declaration changes nothing.
///
/// **Exclusive systems (`add_exclusive_with_access`).** These do not affect
/// scheduling today (the analyzer only walks parallel-stage pairs), so an
/// under-declaration is not a deadlock vector. But their stated purpose
/// (#3473) is to be the thing compared against if either is ever promoted to
/// a parallel lane — and a declaration naming four of the thirteen types its
/// body acquires defeats exactly that, silently, at the moment it would
/// matter most.
///
/// Static source check, matching this file's existing `include_str!`
/// convention: the bodies live in other crates, and running them wants a live
/// `World` with a provider runtime installed.
#[cfg(test)]
mod system_access_declaration_tests {
    const BOOT_SRC: &str = crate::boot::SOURCES;
    const EXECUTE_SRC: &str =
        include_str!("../../../../crates/scripting/src/papyrus_provider/execute.rs");
    const OBSCRIPT_SRC: &str = include_str!("../../../../crates/scripting/src/obscript_runtime.rs");

    const CHARACTER_SRC: &str = include_str!("../../systems/character.rs");
    const CAMERA_SRC: &str = include_str!("../../systems/camera.rs");
    const INTERACTION_SRC: &str = include_str!("../../interaction.rs");
    const TIMER_SRC: &str = include_str!("../../../../crates/scripting/src/timer.rs");
    const ANIMATION_SRC: &str = include_str!("../../systems/animation.rs");
    const CORE_SYSTEMS_SRC: &str = include_str!("../../../../crates/core/src/ecs/systems.rs");
    const PHYSICS_SYNC_SRC: &str = include_str!("../../../../crates/physics/src/sync.rs");
    const AUDIO_SRC: &str = include_str!("../../systems/audio.rs");
    const DEBUG_SRC: &str = include_str!("../../systems/debug.rs");
    const METRICS_SRC: &str = include_str!("../../systems/metrics.rs");
    // #4994 — cross-file hops the within-file follower cannot reach.
    const WATER_SRC: &str = include_str!("../../../../crates/physics/src/water.rs");
    const COMBAT_SRC: &str = include_str!("../../combat.rs");
    const COMBAT_AI_SRC: &str = include_str!("../../systems/combat_ai.rs");
    const LOOT_SRC: &str = include_str!("../../inventory/loot.rs");
    const CRIME_SRC: &str = include_str!("../../crime.rs");
    const ARMOR_SRC: &str = include_str!("../../armor.rs");
    const CRAFTING_SRC: &str = include_str!("../../crafting.rs");
    const LOCKPICK_SRC: &str = include_str!("../../lockpick.rs");
    const SMITHING_SRC: &str = include_str!("../../smithing.rs");
    const AI_PACKAGE_SRC: &str = include_str!("../../npc_spawn/ai_package.rs");
    const ANIM_CONVERT_SRC: &str = include_str!("../../anim_convert.rs");
    // #5307 — the spoken-INFO fragment path: the bin system calls into the
    // scripting crate (apply_spoken_info_fragment → apply_fragment_guard_free),
    // whose deferred-effects flush writes the fragment queues and, on a dirty
    // frame, SceneActorBindings via the scene.rs helper.
    const NPC_DIALOGUE_SRC: &str = include_str!("../../systems/npc_dialogue.rs");
    const FRAGMENT_SYSTEMS_SRC: &str =
        include_str!("../../../../crates/scripting/src/fragment/systems.rs");
    // #5468 — effects.rs is split into effects/*; the scan walks the
    // callees of `apply_fragment_guard_free` (in `deferred.rs`) across the
    // family files, so it reads the whole directory, not one fragment.
    const FRAGMENT_EFFECTS_SRC: &str = concat!(
        include_str!("../../../../crates/scripting/src/fragment/effects.rs"),
        include_str!("../../../../crates/scripting/src/fragment/effects/ai_combat.rs"),
        include_str!("../../../../crates/scripting/src/fragment/effects/deferred.rs"),
        include_str!("../../../../crates/scripting/src/fragment/effects/evaluate.rs"),
        include_str!("../../../../crates/scripting/src/fragment/effects/inventory.rs"),
        include_str!("../../../../crates/scripting/src/fragment/effects/lock.rs"),
        include_str!("../../../../crates/scripting/src/fragment/effects/placement.rs"),
        include_str!("../../../../crates/scripting/src/fragment/effects/player_control.rs"),
        include_str!("../../../../crates/scripting/src/fragment/effects/quest_scoped.rs"),
        include_str!("../../../../crates/scripting/src/fragment/effects/resolve.rs"),
        include_str!("../../../../crates/scripting/src/fragment/effects/scene.rs"),
        include_str!("../../../../crates/scripting/src/fragment/effects/vehicle_cinematic.rs"),
    );
    const SCRIPTING_SCENE_SRC: &str = include_str!("../../../../crates/scripting/src/scene.rs");
    // #5414 — `apply_selection`'s hops out of npc_dialogue.rs: the voice
    // resolver lives in dialogue_voice.rs and the AHEL raise in
    // story_events.rs. Both are invisible to the same-file callee walk,
    // so they are listed explicitly like every other cross-file hop.
    const DIALOGUE_VOICE_SRC: &str = include_str!("../../systems/dialogue_voice.rs");
    const STORY_EVENTS_SRC: &str = include_str!("../../systems/story_events.rs");
    const STORY_MANAGER_SRC: &str = include_str!("../../../../crates/scripting/src/story_manager.rs");
    const OBSCRIPT_STATE_SRC: &str = include_str!("../../../../crates/scripting/src/obscript_state.rs");
    // #5069 — equipment_appearance_system's mid-life gear-import hop lives
    // in npc_spawn/loot_appearance.rs; exclusive rows are invisible to the
    // PARALLEL_SYSTEMS proof, so it gets its own scan below.
    const LOOT_APPEARANCE_SRC: &str = include_str!("../../npc_spawn/loot_appearance.rs");
    // #5649 — the INFO speaker/condition gate routes through the scripting
    // crate's condition evaluator (cross-crate, invisible to the callee
    // walk), and its dispatcher reads a dozen storages/resources the row
    // never declared — including the `StoryManagerRng` write
    // `GetRandomPercent` takes.
    // #5804 — condition.rs is now a hub; the evaluator family it held lives
    // in condition/{eval,context,catalog,requirement}.rs. The concat keeps
    // the callee walk whole across the split (same rule as
    // SCRIPTING_FRAGMENT_SRC above): pointing it at any single file would
    // silently narrow the scan to whichever fragment landed there. catalog.rs
    // goes LAST because its trailing `#[cfg(test)]` rows-walk module is where
    // `production()` splits the text — everything the walk needs precedes it.
    /// The perk compositions combat calls, which lend the weapon subject (PERK-SUB-002).
    const PERK_ENTRY_POINTS_SRC: &str =
        include_str!("../../../../crates/scripting/src/perk_entry_points.rs");
    const CONDITION_SRC: &str = concat!(
        include_str!("../../../../crates/scripting/src/condition/eval.rs"),
        include_str!("../../../../crates/scripting/src/condition/eval/actor.rs"),
        include_str!("../../../../crates/scripting/src/condition/eval/global.rs"),
        include_str!("../../../../crates/scripting/src/condition/eval/reference.rs"),
        include_str!("../../../../crates/scripting/src/condition/context.rs"),
        include_str!("../../../../crates/scripting/src/condition/requirement.rs"),
        include_str!("../../../../crates/scripting/src/condition/catalog.rs"),
    );

    /// #5804 — every production file under `condition/` is in
    /// [`CONDITION_SRC`] (the #3854 rule): a file left out is code the
    /// callee walk stops reading while it still reports green. Walked, not
    /// listed, so the file nobody wrote down is the one it catches.
    #[test]
    fn condition_src_covers_every_condition_file() {
        let this_file = include_str!("mod.rs");
        let (_, concat) = this_file
            .split_once("const CONDITION_SRC: &str = concat!(")
            .expect("the CONDITION_SRC concat! must exist");
        let concat = concat.split_once(");").expect("the concat! is closed").0;
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../crates/scripting/src/condition");
        let mut dirs = vec![root.clone()];
        let mut checked = 0usize;
        while let Some(dir) = dirs.pop() {
            for entry in std::fs::read_dir(&dir).expect("condition/ is readable") {
                let path = entry.expect("readable dir entry").path();
                if path.is_dir() {
                    dirs.push(path);
                    continue;
                }
                let relative = path.strip_prefix(&root).expect("under condition/");
                let relative = relative.to_str().expect("UTF-8 path").replace('\\', "/");
                if !relative.ends_with(".rs") || relative == "tests.rs" {
                    continue;
                }
                assert!(
                    concat.contains(&format!("src/condition/{relative}\")")),
                    "condition/{relative} is missing from CONDITION_SRC"
                );
                checked += 1;
            }
        }
        assert!(checked >= 7, "expected the condition/ evaluator files, walked {checked}");
    }

    /// The nine `add_to_with_access` registrations, each mapped to the
    /// function bodies that make up its acquisition surface.
    ///
    /// Three of them are dispatchers or factories whose own body acquires
    /// almost nothing: `player_controller_system` branches on `PlayerMode`
    /// into `fly_camera_system` / `character_controller_system` (and calls
    /// `refresh_action_state` first), `make_animation_system` is a factory
    /// whose closure calls `animation_system_inner`, and
    /// `physics_sync_system` fans out to same-file phase helpers. Calls are
    /// followed automatically *within a listed file*; a hop into a different
    /// file is listed here explicitly rather than followed, because an
    /// unbounded cross-file follower matches on bare function names and
    /// walks into unrelated code — that over-approximation was measured
    /// during the audit and produced pure noise.
    const PARALLEL_SYSTEMS: &[(&str, &[(&str, &str)])] = &[
        (
            "player_controller_system",
            &[
                (CHARACTER_SRC, "player_controller_system"),
                (CAMERA_SRC, "fly_camera_system"),
                (INTERACTION_SRC, "refresh_action_state"),
                // #4994 — cross-file hops out of character.rs.
                (COMBAT_SRC, "queue_dead_actor_reconciliation"),
                (PHYSICS_SYNC_SRC, "set_kinematic_translation"),
                (PHYSICS_SYNC_SRC, "set_linear_velocity"),
                (WATER_SRC, "weather_wave_adjustment"),
                // #5641 — the drowning death (`character.rs`) raises the KILL
                // story event (#5434): the `StoryEvent` write plus the
                // `resolve_current_lctn` cell-context reads live in
                // story_events.rs, a different file from the callee walk.
                (STORY_EVENTS_SRC, "raise_kill_story_event"),
                // #6447 — and `raise_kill_story_event` funnels into the scripting
                // crate's `raise_story_event`, whose collision arm appends to
                // `PendingScriptStoryEvents` (no turbofish at the call site).
                (STORY_MANAGER_SRC, "raise_story_event"),
                // #6217 — `raise_death_event` funnels into the scripting crate's
                // `note_death` (d1b298921): the kill record write and the victim's
                // identity reads.
                (OBSCRIPT_STATE_SRC, "note_death"),
            ],
        ),
        ("timer_tick_system", &[(TIMER_SRC, "timer_tick_system")]),
        (
            "make_animation_system",
            &[
                (ANIMATION_SRC, "animation_system_inner"),
                // #4994 — the subtree name map walks `Children`.
                (ANIM_CONVERT_SRC, "build_subtree_name_map"),
            ],
        ),
        (
            "make_transform_propagation_system",
            &[(CORE_SYSTEMS_SRC, "make_transform_propagation_system")],
        ),
        (
            "physics_sync_system",
            &[
                (PHYSICS_SYNC_SRC, "physics_sync_system"),
                // #4994 — Phase 2.5 buoyancy lives in water.rs; the eleven
                // water / wind / ragdoll types it takes were invisible here.
                (WATER_SRC, "apply_buoyancy"),
            ],
        ),
        (
            "camera_follow_system",
            &[(CHARACTER_SRC, "camera_follow_system")],
        ),
        ("reverb_zone_system", &[(AUDIO_SRC, "reverb_zone_system")]),
        ("log_stats_system", &[(DEBUG_SRC, "log_stats_system")]),
        (
            "metrics_sample_system",
            &[(METRICS_SRC, "metrics_sample_system")],
        ),
    ];

    /// Production text only. Splitting on a bare `#[cfg(test)]` truncates at
    /// the first `#[cfg(test)] use` / `#[cfg(test)] fn` instead — which in
    /// `systems/animation.rs` sits at line 21 and would hide the entire
    /// file, so the scan would silently find nothing.
    fn production(src: &str) -> &str {
        byroredux_core::source_scan::production_text_or_whole(src)
    }

    /// `boot.rs` up to this module, so the module's own mentions of a system
    /// name cannot be what a scan finds.
    fn boot_setup() -> &'static str {
        BOOT_SRC
            .split("mod system_access_declaration_tests")
            .next()
            .expect("split always yields a first segment")
    }

    /// Balanced-paren slice of the `add_*_with_access(` call that registers
    /// `system`.
    ///
    /// #4064 — anchored on the call's *registration target* (the argument
    /// after `Stage::X`), not on the first textual occurrence of the name.
    /// The old `setup.find(system)` would happily land on a `use` import or
    /// a comment: `player_controller_system`'s block names
    /// `physics_sync_system` in prose, and anchoring on that mention
    /// compared one system's acquisitions against another's declaration.
    fn declaration(system: &str) -> String {
        let setup = boot_setup();
        // Needle composed at runtime so this function's own source text
        // cannot satisfy the scan it performs.
        let open = format!("_with_access{}", "(");
        let mut cursor = 0usize;
        while let Some(found) = setup[cursor..].find(&open) {
            let paren = cursor + found + open.len() - 1;
            let mut depth = 0usize;
            let mut end = paren;
            for (offset, ch) in setup[paren..].char_indices() {
                match ch {
                    '(' => depth += 1,
                    ')' => {
                        depth -= 1;
                        if depth == 0 {
                            end = paren + offset;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            let block = &setup[paren..=end];
            let target = block
                .lines()
                .skip(1)
                .map(str::trim)
                .find(|line| {
                    !line.is_empty() && !line.starts_with("Stage::") && !line.starts_with("//")
                })
                .unwrap_or("")
                .trim_end_matches(',')
                .trim_end_matches("()");
            if target.rsplit("::").next() == Some(system) {
                return block.to_owned();
            }
            cursor = paren + 1;
        }
        panic!("{system}: no `add_*_with_access` registration found in boot/schedule/");
    }

    /// Body of `name` in `src`, delimited by the closing brace at the
    /// function's own indentation.
    fn fn_body<'a>(src: &'a str, name: &str) -> Option<&'a str> {
        let start = [format!("fn {name}("), format!("fn {name}<")]
            .iter()
            .filter_map(|needle| src.find(needle.as_str()))
            .min()?;
        let line_start = src[..start].rfind('\n').map_or(0, |i| i + 1);
        let indent = &src[line_start..start];
        let indent = if indent.trim().is_empty() { indent } else { "" };
        let terminator = format!("\n{indent}}}");
        let end = src[start..]
            .find(&terminator)
            .map_or(src.len(), |i| start + i + terminator.len());
        Some(&src[start..end])
    }

    /// Every storage/resource type acquired inside `body`.
    fn acquired_in(body: &str, types: &mut Vec<String>) {
        // #6362 — `PlayerEntity::get(world)` is the one read of that resource
        // and lives in another file, so the turbofish scan below cannot see
        // it. A call is the acquisition. The needle is composed at run time so
        // this file does not contain it, and a longer name that merely ends in
        // it (`PapyrusPlayerEntity::get`) is a different resource.
        let accessor = format!("{}{}", "PlayerEntity::", "get(");
        let reads_player = body.match_indices(accessor.as_str()).any(|(at, _)| {
            !body[..at].ends_with(|c: char| c.is_alphanumeric() || c == '_')
        });
        if reads_player
            && !types.iter().any(|t| t == "PlayerEntity=read" || t == "PlayerEntity=write")
        {
            types.push("PlayerEntity=read".to_owned());
        }
        // `.query_mut::<crate::Foo>()` -> `Foo`. Needle composed at runtime.
        // #4573 — mode is part of the record: a WRITE acquisition
        // (query_mut / resource_mut / try_resource_mut) must be declared
        // with `.writes::<T>()` specifically, not merely named.
        let open = format!("{}{}", "::", "<");
        for (index, _) in body.match_indices(open.as_str()) {
            let before = &body[..index];
            // #4994 — `world.get::<T>` / `world.has::<T>` take the storage
            // read lock too, and `get_mut::<T>` a write; the scan used to
            // see only the query/resource forms.
            // #4821 — the ambient/death teardown helpers take the storage
            // through a generic `query_mut::<T>` the scan cannot resolve
            // (`T` is the helper's own parameter), so their turbofish call
            // sites are the acquisition.
            let generic_write =
                before.ends_with("remove_component") || before.ends_with("insert_component");
            let is_write = before.ends_with("query_mut")
                || before.ends_with("resource_mut")
                || before.ends_with("try_resource_mut")
                || before.ends_with(".get_mut")
                || generic_write;
            let is_acquire = ["query", "query_mut", "resource", "resource_mut"]
                .iter()
                .any(|form| before.ends_with(form) || before.ends_with(&format!("try_{form}")))
                || [".get", ".get_mut", ".has"]
                    .iter()
                    .any(|form| before.ends_with(form))
                || generic_write;
            if !is_acquire {
                continue;
            }
            let tail = &body[index + open.len()..];
            let Some(close) = tail.find('>') else {
                continue;
            };
            let path = &tail[..close];
            // `$Comp` is a macro metavariable, not a type.
            if path.is_empty() || path.contains(' ') || path.starts_with('$') {
                continue;
            }
            let short = path.rsplit("::").next().unwrap_or(path).to_owned();
            // #4573 — a write subsumes a read of the same type; record the
            // strongest mode. ("write" sorts after "read" lexicographically
            // is NOT relied on; explicit check.)
            let entry = if is_write {
                format!("{short}=write")
            } else {
                format!("{short}=read")
            };
            let read_form = format!("{short}=read");
            let write_form = format!("{short}=write");
            if types.contains(&entry)
                || (!is_write && types.contains(&write_form))
            {
                continue;
            }
            if is_write && types.contains(&read_form) {
                let at = types.iter().position(|t| *t == read_form).unwrap();
                types[at] = entry;
            } else {
                types.push(entry);
            }
        }
    }

    /// Acquisitions of `entry` plus every function it calls that is defined
    /// in the same file, to `MAX_DEPTH` hops.
    fn acquired(src: &str, entry: &str) -> Vec<String> {
        const MAX_DEPTH: usize = 3;
        const NOT_CALLS: &[&str] = &[
            "if", "match", "for", "while", "fn", "return", "assert", "panic",
        ];
        let src = production(src);
        let mut types: Vec<String> = Vec::new();
        let mut seen: Vec<String> = Vec::new();
        let mut pending: Vec<(String, usize)> = vec![(entry.to_owned(), 0)];
        while let Some((name, depth)) = pending.pop() {
            if seen.contains(&name) {
                continue;
            }
            seen.push(name.clone());
            let Some(body) = fn_body(src, &name) else {
                continue;
            };
            acquired_in(body, &mut types);
            if depth == MAX_DEPTH {
                continue;
            }
            for (index, _) in body.match_indices('(') {
                let head = &body[..index];
                // Char-aware scan: a non-ASCII delimiter (comments use ·, —,
                // θ, …) is multi-byte, so advancing a raw byte index by 1
                // lands mid-character and panics the slice. Step by the
                // found char's own length instead.
                let callee_start = head
                    .char_indices()
                    .rev()
                    .find(|(_, c)| !(c.is_ascii_alphanumeric() || *c == '_'))
                    .map_or(0, |(i, c)| i + c.len_utf8());
                let callee = &head[callee_start..];
                if callee.is_empty()
                    || !callee.starts_with(|c: char| c.is_ascii_lowercase() || c == '_')
                    || NOT_CALLS.contains(&callee)
                    || callee == name
                {
                    continue;
                }
                if fn_body(src, callee).is_some() {
                    pending.push((callee.to_owned(), depth + 1));
                }
            }
        }
        types
    }

    fn assert_declares_everything_it_acquires(sources: &[(&str, &str)], system: &str) {
        let mut types: Vec<String> = Vec::new();
        for (src, entry) in sources {
            for ty in acquired(src, entry) {
                if !types.contains(&ty) {
                    types.push(ty);
                }
            }
        }
        assert!(
            types.len() > 1,
            "{system}: the acquisition scan found only {types:?} — the \
             extraction broke, not the declaration"
        );
        let declared = declaration(system);
        // #4573 — strip `//` comment lines before matching, match whole
        // identifiers (a `Transform` acquisition is no longer satisfied by
        // `.reads::<GlobalTransform>()`), and require a WRITE acquisition
        // to be declared with `.writes::<T>()` specifically.
        let declared_code: Vec<&str> = declared
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect();
        let declared_code = declared_code.join("\n");
        let missing: Vec<&String> = types
            .iter()
            .filter(|ty| {
                let (name, mode) = ty
                    .split_once('=')
                    .unwrap_or((ty.as_str(), "read"));
                // Both checks are path-suffix aware: a declaration may
                // spell the type as a full path
                // (`.writes::<byroredux_physics::CharacterController>()`),
                // so match on the line's last path segment.
                let named = declared_code
                    .lines()
                    .any(|line| line.contains("::<") && {
                        let t = line.trim();
                        let inner = t
                            .trim_start_matches(".reads_resource")
                            .trim_start_matches(".writes_resource")
                            .trim_start_matches(".reads")
                            .trim_start_matches(".writes")
                            .trim_start_matches("::<")
                            .trim_end_matches(',')
                            .trim_end_matches(">()");
                        inner.rsplit("::").next() == Some(name)
                    });
                let write_declared = declared_code
                    .lines()
                    .any(|line| {
                        let t = line.trim();
                        if !t.starts_with(".writes_resource") && !t.starts_with(".writes") {
                            return false;
                        }
                        let inner = t
                            .trim_start_matches(".writes_resource")
                            .trim_start_matches(".writes")
                            .trim_start_matches("::<")
                            .trim_end_matches(',')
                            .trim_end_matches(">()");
                        inner.rsplit("::").next() == Some(name)
                    });
                match mode {
                    "write" => !named || !write_declared,
                    _ => !named,
                }
            })
            .collect();
        assert!(
            missing.is_empty(),
            "{system} acquires {missing:?} without declaring them (name=mode; \
             a `=write` entry also requires a .writes::<T> declaration). For a \
             parallel system this makes `install_runtime_registries`'s \
             `known_conflict_count() == 0` unsound — the analyzer cannot see \
             a conflict on a type nobody declared. For an exclusive it is the \
             comparison basis for promoting the system to a parallel lane, \
             and an under-declaration makes that promotion look safe when it \
             is not (#3951/#3473/#4064/#4573)"
        );
    }

    /// #4994 — the cross-file hops and the `get`/`has` forms are only worth
    /// something if the scan actually reaches through them. Pin one type per
    /// hop that no listed same-file body acquires, so dropping a hop (or the
    /// `get` recognition) fails here instead of silently narrowing the guard.
    #[test]
    fn cross_file_hops_and_get_forms_reach_their_acquisitions() {
        let surface = |system: &str| -> Vec<String> {
            let (_, sources) = PARALLEL_SYSTEMS
                .iter()
                .find(|(name, _)| *name == system)
                .unwrap_or_else(|| panic!("{system} left PARALLEL_SYSTEMS"));
            let mut types = Vec::new();
            for (src, entry) in sources.iter() {
                for ty in acquired(src, entry) {
                    if !types.contains(&ty) {
                        types.push(ty);
                    }
                }
            }
            types
        };
        let physics = surface("physics_sync_system");
        assert!(
            physics.iter().any(|t| t == "WaterContact=write"),
            "physics_sync_system's buoyancy hop (water.rs apply_buoyancy) is no longer \
             scanned: {physics:?}"
        );
        let player = surface("player_controller_system");
        for needle in ["PhysicsWorld=write", "PendingDeathReconciliations=write", "WindField=read"] {
            assert!(
                player.iter().any(|t| t == needle),
                "player_controller_system's scan lost `{needle}` — a cross-file hop \
                 (physics set_* helpers / combat.rs / water.rs) was dropped: {player:?}"
            );
        }
        assert!(
            player.iter().any(|t| t == "ActorVitals=read" || t == "ActorVitals=write"),
            "the scan no longer sees `world.get::<T>` acquisitions: {player:?}"
        );
        let animation = surface("make_animation_system");
        assert!(
            animation.iter().any(|t| t.starts_with("Children=")),
            "make_animation_system's anim_convert.rs hop is no longer scanned: {animation:?}"
        );
    }

    #[test]
    fn every_parallel_system_declares_everything_it_acquires() {
        for (system, sources) in PARALLEL_SYSTEMS {
            assert_declares_everything_it_acquires(sources, system);
        }
    }

    /// The table above is only a proof for what it lists. A tenth parallel
    /// registration must not be able to land outside it.
    #[test]
    fn the_parallel_system_table_covers_every_parallel_registration() {
        let setup = boot_setup();
        // Needle composed at runtime — see `declaration`.
        let needle = format!("add_to_with_access{}", "(");
        let registered = setup.matches(needle.as_str()).count();
        assert_eq!(
            registered,
            PARALLEL_SYSTEMS.len(),
            "boot/schedule/ has {registered} `add_to_with_access` registrations but \
             PARALLEL_SYSTEMS lists {}. Every parallel system's declaration is \
             load-bearing for the boot deadlock proof, so a new one must be \
             added to the table with the function bodies that make up its \
             acquisition surface (#4064)",
            PARALLEL_SYSTEMS.len()
        );
    }

    /// #4821 — `npc_combat_ai_system` suspends each attacker's ambient
    /// package through `clear_ambient_behavior`, which removes every ambient
    /// Behavior/State pair through the generic `remove_component::<T>`. Its
    /// declaration claimed to cover them and declared none.
    #[test]
    fn npc_combat_ai_system_declares_everything_it_acquires() {
        assert_declares_everything_it_acquires(
            &[
                (COMBAT_AI_SRC, "npc_combat_ai_system_inner"),
                (PERK_ENTRY_POINTS_SRC, "compose_outgoing_damage"),
                (AI_PACKAGE_SRC, "suspend_ambient_behavior_for_combat"),
            ],
            "make_npc_combat_ai_system",
        );
    }

    /// #6546 — M43 made the busiest gameplay exclusives acquire more than their rows said:
    /// `interaction_system` reads the crafting book, the pick item, the terminal bases, the Story Manager tree
    /// and the ledger and writes `CraftingRequest` and (through `unlock_with_carried_key`) `StoryEvent`;
    /// `container_loot_system` writes `StoryEvent` (`AIPL`); `combat_damage_system` reads the armor model's
    /// tables (`armor::mitigate`) and writes `StoryEvent` (`ASSU`, `KILL`). Exclusives are not paired by the
    /// analyzer, so only this scan keeps the `sys.accesses` report honest for them (#3473).
    #[test]
    fn interaction_system_declares_everything_it_acquires() {
        assert_declares_everything_it_acquires(
            &[
                (INTERACTION_SRC, "interaction_system"),
                (CRAFTING_SRC, "request_crafting"),
                // `unlock_with_carried_key` -> the pick / hack bypass and its `LOCK` / `HACK` events.
                (LOCKPICK_SRC, "bypass_of"),
                (LOCKPICK_SRC, "raise_lockpick_event"),
                (LOCKPICK_SRC, "raise_hack_event"),
                (STORY_MANAGER_SRC, "raise_story_event"),
            ],
            "interaction_system",
        );
    }

    #[test]
    fn container_loot_system_declares_everything_it_acquires() {
        assert_declares_everything_it_acquires(
            &[
                (LOOT_SRC, "container_loot_system"),
                (CRIME_SRC, "report_theft"),
                // `AIPL` (loot.rs) is raised through the Story Manager.
                (STORY_MANAGER_SRC, "raise_story_event"),
            ],
            "container_loot_system",
        );
    }

    #[test]
    fn combat_damage_system_declares_everything_it_acquires() {
        assert_declares_everything_it_acquires(
            &[
                (COMBAT_SRC, "combat_damage_system"),
                (PERK_ENTRY_POINTS_SRC, "compose_damage_threshold"),
                // `armor::mitigate` -> `totals` -> the smithing quality of each worn piece.
                (ARMOR_SRC, "mitigate"),
                (SMITHING_SRC, "stack_quality"),
                // `ASSU` / `KILL`.
                (STORY_MANAGER_SRC, "raise_story_event"),
            ],
            "combat_damage_system",
        );
    }

    #[test]
    fn papyrus_provider_system_declares_everything_it_acquires() {
        assert_declares_everything_it_acquires(
            &[(EXECUTE_SRC, "papyrus_provider_system")],
            "papyrus_provider_system",
        );
    }

    #[test]
    fn legacy_obscript_load_order_system_declares_everything_it_acquires() {
        assert_declares_everything_it_acquires(
            &[(OBSCRIPT_SRC, "legacy_obscript_load_order_system")],
            "legacy_obscript_load_order_system",
        );
    }

    /// #5307 — the selection system's spoken-INFO path writes the fragment
    /// queues (`apply_fragment_guard_free` → `apply_effects` /
    /// `apply_at_depth` take `FragmentExecutionQueue` and
    /// `PendingFragmentActivations` with `try_resource_mut`) and marks
    /// `SceneActorBindings` dirty (`mark_scene_actor_bindings_dirty`), but
    /// the row declared all three as reads. The scan follows the bin system
    /// into the scripting crate so the row's write declarations can no
    /// longer silently rot back to reads.
    ///
    /// #5414 — `apply_selection`'s two cross-file hops are listed too:
    /// `play_line_voice` (dialogue_voice.rs; #5382 VoiceSoundCache +
    /// AudioWorld writes, SoundArchiveProvider/LoadedPluginSet/
    /// GlobalFormIdResolver/GlobalTransform reads, plus the AudioWorld
    /// read of the device gate) and
    /// `raise_hello_story_event` (story_events.rs; the `StoryEvent` write
    /// plus the cell-context reads). Without the explicit entries the
    /// same-file callee walk never sees either and the row can rot again.
    ///
    /// #5649 — the condition evaluator is the third cross-crate hop:
    /// `evaluate_function` (condition.rs) is the dispatcher behind the
    /// INFO speaker/condition gate and acquires the subject-identity
    /// storages, the quest/scene/global ruleset resources, and the
    /// `GetRandomPercent` `StoryManagerRng` write — none of which the row
    /// declared before.
    /// #5069 — `equipment_appearance_system` calls `queue_midlife_imports`
    /// (mid-life gear imports, 0182fc5e8) whose acquisitions the row never
    /// declared: PendingGearImport read+write, ActorBodyClass read, and the
    /// LoadedCellIndex resource. Exclusive, so nothing mis-schedules today —
    /// the row is the promotion baseline and the `sys.accesses` operator
    /// view. Pinned with the same source-scan the other exclusive rows use.
    #[test]
    fn equipment_appearance_system_declares_everything_it_acquires() {
        assert_declares_everything_it_acquires(
            &[(LOOT_APPEARANCE_SRC, "equipment_appearance_system")],
            "equipment_appearance_system",
        );
    }

    #[test]
    fn npc_dialogue_selection_declares_everything_the_spoken_fragment_path_acquires() {
        assert_declares_everything_it_acquires(
            &[
                (NPC_DIALOGUE_SRC, "npc_dialogue_selection_system_inner"),
                (FRAGMENT_SYSTEMS_SRC, "apply_spoken_info_fragment"),
                (FRAGMENT_EFFECTS_SRC, "apply_fragment_guard_free"),
                (SCRIPTING_SCENE_SRC, "mark_scene_actor_bindings_dirty"),
                (DIALOGUE_VOICE_SRC, "play_line_voice"),
                (STORY_EVENTS_SRC, "raise_hello_story_event"),
                (CONDITION_SRC, "evaluate_function"),
            ],
            "make_npc_dialogue_selection_system",
        );
    }
}
