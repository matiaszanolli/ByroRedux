//! Papyrus-demo dispatchers, quest / story / scene playback, package fragments, and the ObScript quest and dialogue lanes.
//!
//! A contiguous run of `Stage::Update` registrations (#6359): registration order is the
//! contract, so [`super::register_update_systems`] calls the segments in file order.

use super::*;

/// M43 — the `ADIA` producer (NPC-to-NPC random conversations). Registered ahead of the Story Manager
/// dispatch so the event is walked the same frame; `BYRO_NO_RANDOM_CONVERSATIONS=1` leaves it out,
/// like the locomotion kill switch.
fn register_random_conversations(scheduler: &mut Scheduler) {
    // `REMP`: a dropped item's event waits for its reference to spawn, so it is polled each frame.
    scheduler.add_exclusive(Stage::Update, crate::inventory::dropped_item_story_event_system);
    // `DEAD`: an actor that sees a corpse discovers it (idle when the tree has no `DEAD` root).
    scheduler.add_exclusive(Stage::Update, crate::systems::dead_body::dead_body_discovery_system);
    // `IRON`: the player begins aiming a gun (idle when the tree has no `IRON` root).
    scheduler.add_exclusive(Stage::Update, crate::systems::iron_sights::iron_sights_system);
    // `TMEE`: placed proximity mines trip, burn their fuse and blast (idle when the load order has no mines).
    scheduler.add_exclusive(Stage::Update, crate::systems::mines::mine_system);
    // `AOBJ`: an actor near an object that shares an attraction keyword reacts (idle without an `AOBJ` root).
    scheduler.add_exclusive(Stage::Update, crate::systems::attraction::attraction_system);
    if std::env::var_os("BYRO_NO_RANDOM_CONVERSATIONS").is_none() {
        scheduler.add_exclusive(Stage::Update, crate::systems::random_conversation::random_conversation_system);
        // The hold system walks / releases the pairs it takes over; `BYRO_NO_AI_LOCOMOTION=1` leaves the
        // walk out (the event is then raised from where the initiator stands).
        let walk = std::env::var_os("BYRO_NO_AI_LOCOMOTION").is_none();
        scheduler.add_exclusive(Stage::Update, crate::systems::random_conversation::make_conversation_hold_system(walk));
    }
}

/// Papyrus-demo dispatchers, quest / story / scene playback, package fragments, and the ObScript quest and dialogue lanes.
pub(super) fn register_quest_story_scene_and_packages(scheduler: &mut Scheduler) {
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
    // #5366 — Story Manager producers + dispatch. The wrapper shapes
    // exist so the registrations below read as a pair: the CLOC
    // producer raises the marker, the dispatcher drains and walks the
    // tree in the same frame.
    fn story_change_location_dispatch(world: &World, _dt: f32) {
        crate::systems::story_events::story_change_location_system(world)
    }
    fn story_manager_dispatch(world: &World, _dt: f32) {
        // #5366 Phase 3 — sync the Story Manager's clock from the
        // canonical game time (advanced by weather_system in Early)
        // before dispatch reads it for RNAM reset windows.
        if let Some(time) = world.try_resource::<crate::components::GameTimeRes>() {
            if let Some(mut clock) =
                world.try_resource_mut::<byroredux_scripting::StoryClock>()
            {
                clock.hours = time.day as f64 * 24.0 + f64::from(time.hour);
            }
        }
        byroredux_scripting::story_manager_dispatch_system(world)
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
    // Offscreen scene actors can reach cataloged trigger REFRs whose cells
    // are not resident. Emit their logical OnTriggerEnter handoff here so
    // quest_advance consumes it in the same frame.
    scheduler.add_exclusive(
        Stage::Update,
        crate::systems::make_scene_trigger_actor_approach_system(),
    );
    scheduler.add_exclusive(Stage::Update, quest_advance_dispatch);
    // ESM data is installed before the player/event sink exists. Bootstrap
    // Start Game Enabled quests here so Begin On Quest Start scenes observe
    // the same-frame transition.
    scheduler.add_exclusive(Stage::Update, byroredux_scripting::quest_startup_system);
    // #5366 — Story Manager: raise CLOC on session-location change, then
    // dispatch every raised story event. Runs right after quest_startup
    // and before the alias refresh so an SM-started quest is
    // indistinguishable from a Start Game Enabled one to everything
    // downstream (aliases, scenes, fragments) — and the KILL marker the
    // combat damage system raises is drained here too, one frame later
    // at the latest.
    scheduler.add_exclusive(Stage::Update, story_change_location_dispatch);
    register_random_conversations(scheduler);
    scheduler.add_exclusive(Stage::Update, story_manager_dispatch);
    // Quest aliases are a general QUST facility, not a SCEN implementation
    // detail. Refresh after startup/cell candidate changes even when the load
    // order contains no scene records; the dirty fast path is allocation-free.
    scheduler.add_exclusive(
        Stage::Update,
        byroredux_scripting::quest_alias_refresh_system,
    );
    scheduler.add_exclusive(
        Stage::Update,
        byroredux_scripting::quest_alias_readiness_stage_system,
    );
    // SCEN playback must observe the original quest-start batch before the
    // fragment dispatcher can replace the shared sink with chained SetStage
    // advances. Phase conditions affected by those fragments are retried on
    // the following tick.
    scheduler.add_exclusive(Stage::Update, byroredux_scripting::scene_playback_system);
    // Execute SCEN VMAD begin/end/phase fragments immediately after playback
    // emits them. Any SetStage effect enters the canonical quest journal for
    // the quest-fragment dispatcher later in this same update.
    scheduler.add_exclusive(
        Stage::Update,
        byroredux_scripting::scene_fragment_dispatch_system,
    );
    // M42.9 / #2652 — ambient packages observe EvaluatePackageRequest before
    // the SCEN package system drains the transient marker. Time-driven checks
    // are bounded to one pass per in-game minute inside the system.
    scheduler.add_exclusive(Stage::Update, crate::npc_spawn::ambient_ai_package_system);
    // M47.2 — the package transitions it reports run their PF_ fragments.
    scheduler.add_exclusive(Stage::Update, byroredux_scripting::package_fragment_system);
    // M47.2 — the perk entry runs the host reports run their PRKF_ fragments.
    scheduler.add_exclusive(Stage::Update, byroredux_scripting::perk_fragment_system);
    // M47.2 — the terminal menu picks the host reports run their TERM_ fragments.
    scheduler.add_exclusive(Stage::Update, byroredux_scripting::terminal_fragment_system);
    // #5367 Phase F — force-greet directives (Dialogue-procedure
    // packages) walk their NPCs to the player and open the conversation
    // without activation. After the ambient package system so a fresh
    // install greets on the next tick; before the Late dialogue
    // selection so the opened surface serials once.
    //
    // #5430 — the approach step is AI motion, so it honours the
    // `BYRO_NO_AI_LOCOMOTION` kill switch; the in-radius open does not.
    // M47.3 — `StartConversation player <topic>` requests become directives first.
    scheduler.add_exclusive(Stage::Update, crate::systems::forcegreet::scripted_conversation_system);
    scheduler.add_exclusive(
        Stage::Update,
        crate::systems::forcegreet::make_forcegreet_system(super::ai_locomotion_enabled()),
    );
    // M47.3 — legacy ObScript quest scripts (Oblivion/FO3/FNV): run each
    // running quest's GameMode block on the vanilla 5 s cadence. Exclusive
    // beside `ambient_ai_package_system`: it drives `QuestStageState`
    // through the same stage events the Papyrus fragment path emits, and
    // the timer maps must stay race-free.
    scheduler.add_exclusive(
        Stage::Update,
        byroredux_scripting::obscript_quests::obscript_quest_tick_system,
    );
    // M47.3 — the result scripts of the stage items set since the last run
    // (by a quest script above, a dialogue line's script last frame, or the
    // console), then of the stages they set. Its own quest-event subscriber.
    scheduler.add_exclusive(
        Stage::Update,
        byroredux_scripting::obscript_results::obscript_stage_result_system,
    );
    // PACK actions resolve their Skyrim PKCU template/data inputs, move actors
    // toward authored invisible-marker coordinates for Travel-family leaves,
    // and queue Done completions for scene playback's next tick.
    scheduler.add_exclusive(Stage::Update, byroredux_scripting::scene_package_system);
    // Package Activate leaves emit the same canonical OnActivate marker as
    // player interaction — but they QUEUE it through
    // `PendingFragmentActivations` rather than inserting it here (#3936),
    // because `scene_package_system` above runs after
    // `rumble_on_activate_dispatch` and `quest_advance_dispatch`. The
    // early `fragment_activation_flush_system` delivers it to every
    // consumer next frame. This registration consumes the flushed
    // marker so GetVMScriptVariable phase gates observe the updated
    // two-state activator, before end-of-frame event cleanup.
    scheduler.add_exclusive(
        Stage::Update,
        byroredux_scripting::two_state_activator_system,
    );
    // Dialogue consumes the ActionStarted batch emitted immediately above,
    // exposes authored INFO subtitle/presentation state for the rest of the
    // frame, and queues completions for scene playback's next tick.
    scheduler.add_exclusive(Stage::Update, byroredux_scripting::scene_dialogue_system);
    // M47.3 — legacy ObScript object scripts (the `SCRI` scripts on placed
    // references): `GameMode` every frame, `OnLoad` and `OnDeath` on their
    // markers. Before `reference_script_event_system` below, which drains
    // `OnDeathEvent` at its head; after the package, activation and dialogue
    // systems whose state the commands read.
    // M47.3 — a cell that reset as the player entered it owes its scripted references an `OnReset`.
    scheduler.add_exclusive(Stage::Update, byroredux_scripting::obscript_state::mark_cell_reset);
    scheduler.add_exclusive(
        Stage::Update,
        byroredux_scripting::obscript_objects::obscript_object_system,
    );
    // M47.3 — the lines `Say` / `SayTo` made actors say: the voice player picks up this
    // frame's, then the timers tick and finished lines run their `SayToDone` blocks.
    scheduler.add_exclusive(Stage::Update, crate::systems::dialogue_voice::scripted_line_voice_system);
    scheduler.add_exclusive(Stage::Update, byroredux_scripting::obscript_speech::obscript_speech_system);
}
