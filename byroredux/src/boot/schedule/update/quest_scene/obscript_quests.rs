//! ObScript quest ticks and stage results.
//!
//! Split out of `quest_scene.rs` (#6704); the rows, their order and their comments are unchanged.

use byroredux_core::ecs::{Scheduler, Stage};

/// ObScript quest ticks and stage results.
pub(super) fn register_obscript_quests_systems(scheduler: &mut Scheduler) {
    // M47.3 — legacy ObScript quest scripts (Oblivion/FO3/FNV): run each
    // running quest's GameMode block on the vanilla 5 s cadence. Exclusive
    // beside `ambient_ai_package_system`: it drives `QuestStageState`
    // through the same stage events the Papyrus fragment path emits, and
    // the timer maps must stay race-free.
    scheduler.add_exclusive(
        Stage::Update,
        byroredux_scripting::obscript_quests::obscript_quest_tick_system,
    );
    // GUI-EVT-006 — the quest scripts' `Begin MenuMode [code]` blocks, on the
    // frames a menu is open. The paused frames a native page holds run it by
    // hand (the paused branch in `app_events.rs`), since the scheduler is skipped.
    scheduler.add_exclusive(
        Stage::Update,
        byroredux_scripting::obscript_quests::obscript_quest_menu_mode_system,
    );
    // M47.3 — the result scripts of the stage items set since the last run
    // (by a quest script above, a dialogue line's script last frame, or the
    // console), then of the stages they set. Its own quest-event subscriber.
    scheduler.add_exclusive(
        Stage::Update,
        byroredux_scripting::obscript_results::obscript_stage_result_system,
    );
}
