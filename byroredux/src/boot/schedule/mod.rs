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

#[cfg(test)]
mod fragment_activation_order_tests {
    //! SCR-D6-NEW11-01 / #2654 — `quest_fragment_dispatch` is the last
    //! producer of `ActivateEvent` in `Stage::Update`, but three of the
    //! four consumers are scheduled before it (and it cannot simply move
    //! earlier: it consumes the `QuestStageAdvanced` markers
    //! `quest_advance_dispatch` emits). Fragment activations are therefore
    //! queued and flushed at the head of the next frame — which only works
    //! while `fragment_activation_flush_system` stays registered ahead of
    //! every consumer.
    //!
    //! Static source check, matching this file's existing
    //! `include_str!` convention: the live boot path wants a Vulkan device
    //! and on-disk game data, which is out of `cargo test` scope.

    const BOOT_SRC: &str = crate::boot::SOURCES;

    #[test]
    fn activation_flush_is_scheduled_before_every_activate_event_consumer() {
        let setup = BOOT_SRC
            .split("mod fragment_activation_order_tests")
            .next()
            .expect("split always yields a first segment");

        let pos = |needle: &str| {
            setup
                .find(needle)
                .unwrap_or_else(|| panic!("`{needle}` is no longer registered in boot.rs"))
        };

        // Match the *registration* sites, not the `fn` definitions above them.
        let flush = pos("byroredux_scripting::fragment_activation_flush_system");
        for consumer in [
            "Stage::Update, rumble_on_activate_dispatch)",
            "Stage::Update, quest_advance_dispatch)",
            "byroredux_scripting::two_state_activator_system",
        ] {
            assert!(
                flush < pos(consumer),
                "fragment_activation_flush_system must be scheduled before `{consumer}`, or \
                 fragment `<Ref>.Activate()` silently no-ops again (#2654)"
            );
        }

        // And the producer still has to run after the advance that feeds it.
        assert!(
            pos("Stage::Update, quest_advance_dispatch)")
                < pos("Stage::Update, quest_fragment_dispatch)"),
            "quest_fragment_dispatch consumes QuestStageAdvanced and must stay after \
             quest_advance_dispatch"
        );
    }

    #[test]
    fn extension_event_adapters_run_before_transient_cleanup() {
        let setup = BOOT_SRC
            .split("mod fragment_activation_order_tests")
            .next()
            .expect("split always yields a first segment");
        let pos = |needle: &str| {
            setup
                .rfind(needle)
                .unwrap_or_else(|| panic!("{needle} is no longer registered in boot.rs"))
        };

        let settings = pos("crate::extensions::extension_engine_settings_sync_system");
        let catalog = pos("crate::extensions::extension_content_catalog_sync_system");
        let input_bindings = pos("crate::extensions::extension_input_bindings_sync_system");
        let legacy = pos("byroredux_scripting::legacy_obscript_load_order_system");
        let papyrus = pos("byroredux_scripting::papyrus_provider_system");
        let custom = pos("crate::extensions::extension_custom_event_dispatch_system");
        let activation = pos("crate::extensions::extension_activation_dispatch_system");
        let cell_load = pos("crate::extensions::extension_cell_load_dispatch_system");
        let equipment = pos("crate::extensions::extension_equipment_dispatch_system");
        let input = pos("crate::extensions::extension_input_dispatch_system");
        let session = pos("crate::extensions::extension_session_dispatch_system");
        let hit = pos("crate::extensions::extension_hit_dispatch_system");
        let update = pos("crate::extensions::extension_update_dispatch_system");
        let setting_apply = pos("crate::extensions::extension_setting_write_apply_system");
        let cleanup = pos("byroredux_scripting::event_cleanup_system");
        assert!(
            settings < custom
                && catalog < custom
                && catalog < input_bindings
                && input_bindings < legacy
                && catalog < legacy
                && legacy < cleanup
                && legacy < papyrus
                && papyrus < cleanup
                && custom < activation
                && custom < cleanup
                && activation < cleanup
                && cell_load < cleanup
                && equipment < cleanup
                && input < cleanup
                && session < cleanup
                && hit < cleanup
                && update < setting_apply
                && setting_apply < cleanup,
            "extension callbacks must run before transient marker cleanup"
        );
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
        let setup = BOOT_SRC
            .split("mod fragment_activation_order_tests")
            .next()
            .expect("split always yields a first segment");
        let pos = |needle: &str| {
            setup
                .find(needle)
                .unwrap_or_else(|| panic!("`{needle}` is no longer registered in boot/"))
        };

        let scene_playback = pos("byroredux_scripting::scene_playback_system");
        let scene_fragment = pos("byroredux_scripting::scene_fragment_dispatch_system");
        let quest_fragment = pos("Stage::Update, quest_fragment_dispatch)");
        let continuation = pos("byroredux_scripting::fragment_continuation_system");

        assert!(
            scene_playback < scene_fragment,
            "scene_fragment_dispatch consumes what scene_playback emits"
        );
        assert!(
            scene_fragment < quest_fragment,
            "quest_fragment_dispatch consumes the QuestStageAdvanced markers \
             scene_fragment_dispatch can produce"
        );
        assert!(
            quest_fragment < continuation,
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
        let setup = BOOT_SRC
            .split("mod fragment_activation_order_tests")
            .next()
            .expect("split always yields a first segment");

        let cleanup = setup
            .rfind("byroredux_scripting::event_cleanup_system")
            .expect("event_cleanup_system is no longer registered in boot/");

        // Every `add_exclusive*` call whose stage argument is `Stage::Late`
        // has to sit before it. The stage often lands on the following
        // line for the `_with_access` form, so look ahead past it.
        let mut scan = 0usize;
        let mut checked = 0usize;
        while let Some(rel) = setup[scan..].find("add_exclusive") {
            let at = scan + rel;
            scan = at + "add_exclusive".len();
            let window = &setup[at..setup.len().min(at + 200)];
            let Some(stage_rel) = window.find("Stage::") else {
                continue;
            };
            if !window[stage_rel..].starts_with("Stage::Late") {
                continue;
            }
            checked += 1;
            // No `continue` for `at >= cleanup`: that is precisely the
            // failing case. The cleanup registration's own `add_exclusive`
            // token sits *before* the system path on the same line, so it
            // satisfies this comparison without needing an exemption — and
            // an exemption here would have made the assert unfalsifiable.
            assert!(
                at < cleanup,
                "a Stage::Late exclusive is registered after event_cleanup_system \
                 (byte {at} vs {cleanup}); the markers it drains would be gone \
                 before that system reads them"
            );
        }
        assert!(
            checked > 5,
            "expected to inspect the Late exclusives, found {checked} — the scan \
             is broken, not the schedule"
        );
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

    /// The only `SchedulerSystemTimings` insert in `boot.rs` must sit
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
            "expected exactly one SchedulerSystemTimings insert in boot.rs, found {}: {:?}",
            inserts.len(),
            inserts,
        );

        let gate = setup
            .find("if std::env::var_os(\"BYRO_PROFILE\").is_some() {")
            .expect(
                "boot.rs must gate the SchedulerSystemTimings insert on BYRO_PROFILE — an \
                 unconditional insert re-arms the scheduler tracker every frame (#2166)",
            );
        let insert = setup
            .find("insert_resource(byroredux_core::ecs::SchedulerSystemTimings::default())")
            .expect("SchedulerSystemTimings insert not found in boot.rs");
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
             first opens — boot.rs no longer does it unconditionally (#2166)"
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
    //! the `WindField` read declaration in the same change (`boot.rs`
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
            ],
        ),
        ("timer_tick_system", &[(TIMER_SRC, "timer_tick_system")]),
        (
            "make_animation_system",
            &[(ANIMATION_SRC, "animation_system_inner")],
        ),
        (
            "make_transform_propagation_system",
            &[(CORE_SYSTEMS_SRC, "make_transform_propagation_system")],
        ),
        (
            "physics_sync_system",
            &[(PHYSICS_SYNC_SRC, "physics_sync_system")],
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
        src.split_once("#[cfg(test)]\nmod ")
            .map_or(src, |(before, _)| before)
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
        panic!("{system}: no `add_*_with_access` registration found in boot.rs");
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
        // `.query_mut::<crate::Foo>()` -> `Foo`. Needle composed at runtime.
        let open = format!("{}{}", "::", "<");
        for (index, _) in body.match_indices(open.as_str()) {
            let before = &body[..index];
            let is_acquire = ["query", "query_mut", "resource", "resource_mut"]
                .iter()
                .any(|form| before.ends_with(form) || before.ends_with(&format!("try_{form}")));
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
            if !types.contains(&short) {
                types.push(short);
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
                let callee_start = head
                    .rfind(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                    .map_or(0, |i| i + 1);
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
        let missing: Vec<&String> = types.iter().filter(|ty| !declared.contains(*ty)).collect();
        assert!(
            missing.is_empty(),
            "{system} acquires {missing:?} without declaring them. For a \
             parallel system this makes `install_runtime_registries`'s \
             `known_conflict_count() == 0` unsound — the analyzer cannot see \
             a conflict on a type nobody declared. For an exclusive it is the \
             comparison basis for promoting the system to a parallel lane, \
             and an under-declaration makes that promotion look safe when it \
             is not (#3951/#3473/#4064)"
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
            "boot.rs has {registered} `add_to_with_access` registrations but \
             PARALLEL_SYSTEMS lists {}. Every parallel system's declaration is \
             load-bearing for the boot deadlock proof, so a new one must be \
             added to the table with the function bodies that make up its \
             acquisition surface (#4064)",
            PARALLEL_SYSTEMS.len()
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
}
